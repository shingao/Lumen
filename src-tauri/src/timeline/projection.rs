//! Projection « disque plein dans N semaines » : régression linéaire (moindres carrés) de
//! l'espace utilisé sur les 30 derniers jours, avec au moins 5 points. Les baisses dues aux
//! nettoyages de Lumen ne sont pas une tendance : ce qu'un nettoyage a libéré est rajouté aux
//! points suivants avant la régression. Pente nulle ou négative : pas de projection.

use super::retention::DAY_MS;

pub const WINDOW_DAYS: i64 = 30;
pub const MIN_POINTS: usize = 5;
/// En dessous (octets par jour), la pente est considérée comme nulle.
const FLAT_BYTES_PER_DAY: f64 = 1024.0 * 1024.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub at: i64,
    pub used: u64,
}

/// Nettoyage de Lumen : ce qu'il a libéré dans la série, à sa date.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cleanup {
    pub at: i64,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Projection {
    /// Moins de 5 points sur 30 jours.
    TooFew { points: usize },
    /// Pente nulle ou négative : un message neutre, pas de date.
    Flat { bytes_per_day: f64 },
    /// Croissance : jours restants avant que l'espace libre soit épuisé (si connu).
    Growing { bytes_per_day: f64, days_to_full: Option<f64>, points: usize },
}

/// Points de la fenêtre, corrigés des nettoyages survenus depuis le premier d'entre eux.
pub fn adjusted(points: &[Point], cleanups: &[Cleanup]) -> Vec<(f64, f64)> {
    let Some(first) = points.first() else { return Vec::new() };
    points
        .iter()
        .map(|p| {
            let cleaned: u64 = cleanups.iter().filter(|c| c.at > first.at && c.at <= p.at).map(|c| c.bytes).sum();
            ((p.at - first.at) as f64 / DAY_MS as f64, (p.used + cleaned) as f64)
        })
        .collect()
}

/// Pente (octets par jour) des moindres carrés ; `None` si tous les points ont la même date.
pub fn slope(xy: &[(f64, f64)]) -> Option<f64> {
    let n = xy.len() as f64;
    let (mx, my) = xy.iter().fold((0.0, 0.0), |(a, b), (x, y)| (a + x / n, b + y / n));
    let (num, den) = xy.iter().fold((0.0, 0.0), |(num, den), (x, y)| (num + (x - mx) * (y - my), den + (x - mx) * (x - mx)));
    (den > 0.0).then(|| num / den)
}

/// `points` triés par date ; `free` : espace libre du volume au dernier point.
pub fn project(points: &[Point], cleanups: &[Cleanup], now: i64, free: Option<u64>) -> Projection {
    let window: Vec<Point> = points.iter().copied().filter(|p| p.at >= now - WINDOW_DAYS * DAY_MS && p.at <= now).collect();
    if window.len() < MIN_POINTS {
        return Projection::TooFew { points: window.len() };
    }
    let Some(per_day) = slope(&adjusted(&window, cleanups)) else {
        return Projection::TooFew { points: 1 };
    };
    if per_day < FLAT_BYTES_PER_DAY {
        return Projection::Flat { bytes_per_day: per_day };
    }
    Projection::Growing { bytes_per_day: per_day, days_to_full: free.map(|f| f as f64 / per_day), points: window.len() }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const NOW: i64 = 1_792_584_000_000;

    fn series(days: &[i64], f: impl Fn(i64) -> f64) -> Vec<Point> {
        let mut v: Vec<Point> = days.iter().map(|&d| Point { at: NOW - d * DAY_MS, used: f(d) as u64 }).collect();
        v.sort_by_key(|p| p.at);
        v
    }

    #[test]
    fn croissance_reguliere() {
        // +1 Gio par jour pendant 20 jours, 100 Gio libres : plein dans 100 jours.
        let pts = series(&(0..20).collect::<Vec<_>>(), |d| 500.0 * GIB - d as f64 * GIB);
        match project(&pts, &[], NOW, Some((100.0 * GIB) as u64)) {
            Projection::Growing { bytes_per_day, days_to_full, points } => {
                assert!((bytes_per_day / GIB - 1.0).abs() < 1e-6);
                assert!((days_to_full.unwrap_or(0.0) - 100.0).abs() < 1e-3);
                assert_eq!(points, 20);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn seulement_avec_cinq_points_sur_trente_jours() {
        let four = series(&[0, 5, 10, 15], |d| 100.0 * GIB - d as f64 * GIB);
        assert_eq!(project(&four, &[], NOW, None), Projection::TooFew { points: 4 });
        // Des points plus vieux que 30 jours ne comptent pas.
        let old = series(&[0, 5, 10, 15, 40, 50, 60], |d| 100.0 * GIB - d as f64 * GIB);
        assert_eq!(project(&old, &[], NOW, None), Projection::TooFew { points: 4 });
        let five = series(&[0, 5, 10, 15, 20], |d| 100.0 * GIB - d as f64 * GIB);
        assert!(matches!(project(&five, &[], NOW, None), Projection::Growing { days_to_full: None, .. }));
    }

    #[test]
    fn pente_nulle_ou_negative_pas_de_projection() {
        let flat = series(&(0..10).collect::<Vec<_>>(), |_| 200.0 * GIB);
        assert!(matches!(project(&flat, &[], NOW, Some(1)), Projection::Flat { .. }));
        let shrinking = series(&(0..10).collect::<Vec<_>>(), |d| 200.0 * GIB + d as f64 * GIB);
        match project(&shrinking, &[], NOW, Some(1)) {
            Projection::Flat { bytes_per_day } => assert!(bytes_per_day < 0.0),
            other => panic!("{other:?}"),
        }
        // Quelques octets par jour : nul.
        let tiny = series(&(0..10).collect::<Vec<_>>(), |d| 200.0 * GIB - d as f64 * 1000.0);
        assert!(matches!(project(&tiny, &[], NOW, Some(1)), Projection::Flat { .. }));
    }

    #[test]
    fn sauts_des_nettoyages_exclus() {
        // +0,5 Gio par jour ; un nettoyage de Lumen de 44 Gio il y a 8 jours. Sans correction, la
        // pente serait négative ; corrigée, elle reste à 0,5 Gio par jour.
        let cleanup_at = NOW - 8 * DAY_MS - 3_600_000;
        let pts = series(&(0..25).collect::<Vec<_>>(), |d| {
            let base = 300.0 * GIB - d as f64 * 0.5 * GIB;
            if NOW - d * DAY_MS > cleanup_at { base - 44.0 * GIB } else { base }
        });
        let raw = slope(&adjusted(&pts, &[])).unwrap_or(0.0);
        assert!(raw < 0.0, "sans correction : {raw}");
        let cleanups = [Cleanup { at: cleanup_at, bytes: (44.0 * GIB) as u64 }];
        match project(&pts, &cleanups, NOW, Some((50.0 * GIB) as u64)) {
            Projection::Growing { bytes_per_day, days_to_full, .. } => {
                assert!((bytes_per_day / GIB - 0.5).abs() < 1e-6, "{}", bytes_per_day / GIB);
                assert!((days_to_full.unwrap_or(0.0) - 100.0).abs() < 1e-3);
            }
            other => panic!("{other:?}"),
        }
        // Un nettoyage avant la fenêtre ou après le dernier point ne change rien.
        let outside = [Cleanup { at: NOW - 90 * DAY_MS, bytes: 1 << 40 }, Cleanup { at: NOW + DAY_MS, bytes: 1 << 40 }];
        let a = slope(&adjusted(&pts, &cleanups)).unwrap_or(0.0);
        let mut both = cleanups.to_vec();
        both.extend(outside);
        let b = slope(&adjusted(&pts, &both)).unwrap_or(0.0);
        assert!((a - b).abs() < 1e-3);
    }

    #[test]
    fn serie_connue_avec_bruit() {
        // y = 10 + 2x sur des x irréguliers, bruit symétrique : la pente des moindres carrés est 2.
        let xy = [(0.0, 10.0), (1.0, 12.5), (2.0, 13.5), (4.0, 18.0), (7.0, 24.0), (9.0, 28.0)];
        let s = slope(&xy).unwrap_or(0.0);
        let expected = {
            let n = xy.len() as f64;
            let sx: f64 = xy.iter().map(|p| p.0).sum();
            let sy: f64 = xy.iter().map(|p| p.1).sum();
            let sxy: f64 = xy.iter().map(|p| p.0 * p.1).sum();
            let sxx: f64 = xy.iter().map(|p| p.0 * p.0).sum();
            (n * sxy - sx * sy) / (n * sxx - sx * sx)
        };
        assert!((s - expected).abs() < 1e-9);
        assert!((s - 2.0).abs() < 0.1, "{s}");
        assert_eq!(slope(&[(1.0, 1.0), (1.0, 5.0)]), None);
    }
}
