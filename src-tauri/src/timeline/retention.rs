//! Rétention des snapshots, série par série, pour borner la taille de la base : tous les
//! snapshots des 7 derniers jours, puis un par jour jusqu'à 30 jours, un par semaine jusqu'à
//! 6 mois, un par mois au-delà. Dans chaque case (jour, semaine du lundi, mois civil, en UTC),
//! le plus récent est gardé : appliquer la règle deux fois ne retire rien de plus.

use std::collections::HashMap;

pub const DAY_MS: i64 = 86_400_000;
const KEEP_ALL_DAYS: i64 = 7;
const DAILY_UNTIL_DAYS: i64 = 30;
/// 6 mois.
const WEEKLY_UNTIL_DAYS: i64 = 183;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Bucket {
    Day(i64),
    Week(i64),
    Month(i64),
}

/// Mois civil (année × 12 + mois) d'un jour compté depuis le 1er janvier 1970.
fn month_of(days: i64) -> i64 {
    // Algorithme « civil_from_days » de Howard Hinnant.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    year * 12 + (month - 1)
}

/// Case d'un snapshot selon son âge ; `None` : gardé quoi qu'il arrive (moins de 7 jours).
fn bucket(taken_at: i64, now: i64) -> Option<Bucket> {
    let age = now - taken_at;
    if age < KEEP_ALL_DAYS * DAY_MS {
        return None;
    }
    let day = taken_at.div_euclid(DAY_MS);
    Some(if age < DAILY_UNTIL_DAYS * DAY_MS {
        Bucket::Day(day)
    } else if age < WEEKLY_UNTIL_DAYS * DAY_MS {
        // Le 1er janvier 1970 était un jeudi : +3 fait commencer les semaines le lundi.
        Bucket::Week((day + 3).div_euclid(7))
    } else {
        Bucket::Month(month_of(day))
    })
}

/// Snapshots d'une série (identifiant, date en ms) à supprimer à la date `now`.
pub fn to_delete(snapshots: &[(i64, i64)], now: i64) -> Vec<i64> {
    let mut kept: HashMap<Bucket, (i64, i64)> = HashMap::new();
    let mut out = Vec::new();
    for &(id, at) in snapshots {
        let Some(b) = bucket(at, now) else { continue };
        match kept.get(&b) {
            // Le plus récent reste ; à date égale, le plus grand identifiant (le dernier écrit).
            Some(&(kid, kat)) if (kat, kid) >= (at, id) => out.push(id),
            Some(&(kid, _)) => {
                out.push(kid);
                kept.insert(b, (id, at));
            }
            None => {
                kept.insert(b, (id, at));
            }
        }
    }
    out.sort_unstable();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// 21 octobre 2026, 12 h UTC.
    const NOW: i64 = 1_792_584_000_000;

    #[test]
    fn mois_civils() {
        assert_eq!(month_of(0), 1970 * 12);
        assert_eq!(month_of(31), 1970 * 12 + 1); // 1er février 1970
        assert_eq!(month_of(NOW / DAY_MS), 2026 * 12 + 9); // octobre 2026
        assert_eq!(month_of(-1), 1969 * 12 + 11);
    }

    /// Historique synthétique d'un an : un snapshot à chaque analyse (3 par jour, à heures
    /// variables) plus un quotidien.
    fn one_year() -> Vec<(i64, i64)> {
        let mut v = Vec::new();
        let mut id = 0;
        for day in 0..366 {
            for hour in [1, 9, 13, 22] {
                id += 1;
                v.push((id, NOW - day * DAY_MS - hour * 3_600_000 - (day % 7) * 60_000));
            }
        }
        v
    }

    #[test]
    fn retention_sur_un_an() {
        let all = one_year();
        let gone: HashSet<i64> = to_delete(&all, NOW).into_iter().collect();
        let kept: Vec<(i64, i64)> = all.iter().copied().filter(|(id, _)| !gone.contains(id)).collect();
        let age = |at: i64| NOW - at;

        // Tous ceux des 7 derniers jours.
        let recent = all.iter().filter(|(_, at)| age(*at) < 7 * DAY_MS).count();
        assert_eq!(kept.iter().filter(|(_, at)| age(*at) < 7 * DAY_MS).count(), recent);
        assert_eq!(recent, 4 * 7);

        // Un par jour (le plus récent du jour) entre 7 et 30 jours.
        let days: Vec<i64> = kept.iter().filter(|(_, at)| (7 * DAY_MS..30 * DAY_MS).contains(&age(*at))).map(|(_, at)| at.div_euclid(DAY_MS)).collect();
        assert_eq!(days.len(), days.iter().collect::<HashSet<_>>().len());
        assert!((22..=24).contains(&days.len()), "{}", days.len());
        for (_, at) in kept.iter().filter(|(_, at)| (7 * DAY_MS..30 * DAY_MS).contains(&age(*at))) {
            let later_same_day = all.iter().any(|(_, o)| o.div_euclid(DAY_MS) == at.div_euclid(DAY_MS) && o > at && age(*o) >= 7 * DAY_MS);
            assert!(!later_same_day, "le plus récent du jour est gardé");
        }

        // Un par semaine entre 30 jours et 6 mois.
        let weeks: Vec<i64> = kept.iter().filter(|(_, at)| (30 * DAY_MS..183 * DAY_MS).contains(&age(*at))).map(|(_, at)| (at.div_euclid(DAY_MS) + 3).div_euclid(7)).collect();
        assert_eq!(weeks.len(), weeks.iter().collect::<HashSet<_>>().len());
        assert!((21..=24).contains(&weeks.len()), "{}", weeks.len());

        // Un par mois au-delà.
        let months: Vec<i64> = kept.iter().filter(|(_, at)| age(*at) >= 183 * DAY_MS).map(|(_, at)| month_of(at.div_euclid(DAY_MS))).collect();
        assert_eq!(months.len(), months.iter().collect::<HashSet<_>>().len());
        assert!((6..=8).contains(&months.len()), "{}", months.len());

        // Base bornée : de 1 464 snapshots à environ 80.
        assert!(kept.len() < 90, "{}", kept.len());
        // Le plus ancien mois n'est jamais vidé entièrement.
        let oldest = all.iter().map(|(_, at)| *at).min().unwrap_or(NOW);
        assert!(kept.iter().any(|(_, at)| month_of(at.div_euclid(DAY_MS)) == month_of(oldest.div_euclid(DAY_MS))));

        // Idempotent.
        assert!(to_delete(&kept, NOW).is_empty());
        // Un mois plus tard, sans nouveau snapshot : seule la rétention avance, jamais de trou
        // dans un mois qui avait des snapshots.
        let later = NOW + 30 * DAY_MS;
        let gone2: HashSet<i64> = to_delete(&kept, later).into_iter().collect();
        let kept2: Vec<(i64, i64)> = kept.iter().copied().filter(|(id, _)| !gone2.contains(id)).collect();
        let months_before: HashSet<i64> = kept.iter().map(|(_, at)| month_of(at.div_euclid(DAY_MS))).collect();
        let months_after: HashSet<i64> = kept2.iter().map(|(_, at)| month_of(at.div_euclid(DAY_MS))).collect();
        assert_eq!(months_before, months_after);
    }

    #[test]
    fn rien_a_supprimer_dans_la_semaine() {
        let v: Vec<(i64, i64)> = (0..50).map(|i| (i, NOW - i * 3_600_000)).collect();
        assert!(to_delete(&v, NOW).is_empty());
        assert!(to_delete(&[], NOW).is_empty());
    }
}
