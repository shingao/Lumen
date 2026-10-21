//! Journal des nettoyages : SQLite (`%LOCALAPPDATA%\Lumen\lumen.db`, WAL). Chaque nettoyage
//! garde son bilan, le script équivalent à ce qui a été exécuté, chaque élément (méthode,
//! résultat, taille, raison, emplacement dans la Corbeille pour la restauration) et chaque
//! commande officielle (binaire par son chemin complet, code de sortie, sorties capturées).
//! Il garde aussi le cache des empreintes des doublons (file ID + taille + date de modification).

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use safety::exec::{CommandStatus, ItemOutcome, Report};
use safety::plan::{CommandRole, DeleteMode};

use crate::advanced::duplicates::{CacheEntry, CacheKey};

const SCHEMA_VERSION: i64 = 4;
/// Empreinte de doublon oubliée si elle n'a pas servi depuis (ms).
const HASH_CACHE_TTL_MS: i64 = 180 * 86_400 * 1000;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS actions (
  id INTEGER PRIMARY KEY,
  started_at INTEGER NOT NULL,
  finished_at INTEGER NOT NULL,
  title TEXT NOT NULL,
  method TEXT NOT NULL CHECK (method IN ('trash', 'permanent')),
  plan_hash TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('ok', 'partial', 'failed', 'cancelled')),
  bytes_planned INTEGER NOT NULL,
  bytes_freed INTEGER NOT NULL,
  bytes_measured INTEGER,
  items_done INTEGER NOT NULL,
  items_partial INTEGER NOT NULL,
  items_skipped INTEGER NOT NULL,
  items_refused INTEGER NOT NULL,
  items_failed INTEGER NOT NULL,
  items_cancelled INTEGER NOT NULL,
  script TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS actions_by_date ON actions(started_at);
CREATE TABLE IF NOT EXISTS action_items (
  id INTEGER PRIMARY KEY,
  action_id INTEGER NOT NULL REFERENCES actions(id) ON DELETE CASCADE,
  path TEXT NOT NULL,
  rule_id TEXT,
  directory INTEGER NOT NULL,
  size INTEGER NOT NULL,
  method TEXT NOT NULL,
  result TEXT NOT NULL CHECK (result IN ('done', 'failed', 'left', 'skipped', 'refused', 'cancelled')),
  reason TEXT,
  volume_serial INTEGER,
  file_id TEXT,
  recycle_item TEXT,
  restored_at INTEGER
);
CREATE INDEX IF NOT EXISTS items_by_action ON action_items(action_id);
CREATE TABLE IF NOT EXISTS action_commands (
  id INTEGER PRIMARY KEY,
  action_id INTEGER NOT NULL REFERENCES actions(id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  program TEXT,
  args TEXT NOT NULL,
  status TEXT NOT NULL,
  exit_code INTEGER,
  stdout TEXT NOT NULL,
  stderr TEXT NOT NULL,
  duration_ms INTEGER NOT NULL
);
";

/// v3 : fichier de sortie d'une commande (journal de DISM), actions système lancées sans point
/// de restauration après confirmation explicite.
const SCHEMA_V3: &str = "
ALTER TABLE action_commands ADD COLUMN log_path TEXT;
ALTER TABLE actions ADD COLUMN without_restore_point INTEGER NOT NULL DEFAULT 0;
";

/// v4 (jalon 7) : snapshots de la Timeline. Un volume est identifié par son GUID (à défaut, son
/// numéro de série et son système de fichiers), jamais par sa lettre, qui n'est qu'un dernier état
/// connu. Un snapshot ne garde que des tailles agrégées par dossier (4 niveaux au plus), jamais la
/// liste des fichiers ; les chemins sont relatifs à la racine du snapshot, elle-même relative au
/// volume (sans lettre). Chaque nettoyage retient le volume qu'il visait (repères de la courbe).
const SCHEMA_V4: &str = "
CREATE TABLE IF NOT EXISTS volumes (
  id INTEGER PRIMARY KEY,
  key TEXT NOT NULL UNIQUE,
  guid TEXT,
  serial INTEGER NOT NULL,
  scan_serial INTEGER,
  fs TEXT NOT NULL,
  label TEXT NOT NULL,
  letter TEXT NOT NULL,
  seen_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS snapshots (
  id INTEGER PRIMARY KEY,
  volume_id INTEGER NOT NULL REFERENCES volumes(id) ON DELETE CASCADE,
  root TEXT NOT NULL COLLATE NOCASE,
  method TEXT NOT NULL CHECK (method IN ('walk', 'mft')),
  source TEXT NOT NULL CHECK (source IN ('scan', 'daily')),
  taken_at INTEGER NOT NULL,
  used INTEGER NOT NULL,
  files INTEGER NOT NULL,
  volume_total INTEGER,
  volume_free INTEGER,
  letter TEXT NOT NULL,
  min_bytes INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS snapshots_by_series ON snapshots(volume_id, root, method, taken_at);
CREATE TABLE IF NOT EXISTS snapshot_paths (id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS snapshot_sizes (
  snapshot_id INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
  path_id INTEGER NOT NULL REFERENCES snapshot_paths(id),
  allocated INTEGER NOT NULL,
  files INTEGER NOT NULL,
  PRIMARY KEY (snapshot_id, path_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS snapshot_sizes_by_path ON snapshot_sizes(path_id);
ALTER TABLE actions ADD COLUMN volume_key TEXT;
";

/// Migrations, dans l'ordre : (version atteinte, SQL). La base vide est d'abord créée en v1.
const MIGRATIONS: [(i64, &str); 3] = [(2, SCHEMA_V2), (3, SCHEMA_V3), (4, SCHEMA_V4)];

/// Fin d'un fichier de sortie (journal de DISM) pour l'affichage : 64 Kio au plus, 80 lignes.
pub fn log_tail(path: &str) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(64 * 1024))).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    // DISM écrit en UTF-16 ou en ANSI selon la version : on accepte les deux.
    let text = if buf.iter().skip(1).step_by(2).take(64).filter(|&&b| b == 0).count() > 16 {
        let start = usize::from(buf.len() % 2 == 1);
        String::from_utf16_lossy(&buf[start..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())
    } else {
        String::from_utf8_lossy(&buf).into_owned()
    };
    let lines: Vec<&str> = text.trim_start_matches('\u{feff}').lines().collect();
    Some(lines[lines.len().saturating_sub(80)..].join("\n"))
}

/// v2 (jalon 6) : type de nettoyage, arrêt avant les actions, cache des empreintes.
const SCHEMA_V2: &str = "
ALTER TABLE actions ADD COLUMN kind TEXT NOT NULL DEFAULT 'recos';
ALTER TABLE actions ADD COLUMN aborted TEXT;
CREATE TABLE IF NOT EXISTS hash_cache (
  volume_serial INTEGER NOT NULL,
  file_id TEXT NOT NULL,
  size INTEGER NOT NULL,
  modified INTEGER NOT NULL,
  edges BLOB,
  full BLOB,
  used_at INTEGER NOT NULL,
  PRIMARY KEY (volume_serial, file_id, size, modified)
);
";

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("journal illisible : {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("journal : {0}")]
    Io(#[from] std::io::Error),
    #[error("journal d'une version plus récente de Lumen ({0})")]
    Newer(i64),
}

pub struct Journal {
    conn: Connection,
}

/// Ce que l'on sait d'une exécution en plus de son bilan.
pub struct ActionMeta<'a> {
    /// `recos`, `dormant`, `largest`, `duplicates`, `system`.
    pub kind: &'a str,
    /// Actions système lancées sans point de restauration, après confirmation explicite.
    pub without_restore_point: bool,
    pub started_at: i64,
    pub finished_at: i64,
    pub title: &'a str,
    pub planned_bytes: u64,
    pub script: &'a str,
    /// Volume visé (clé stable de la Timeline : GUID ou numéro de série), pour les repères.
    pub volume_key: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActionRow {
    pub id: i64,
    pub started_at: i64,
    pub finished_at: i64,
    pub title: String,
    pub method: String,
    pub status: String,
    pub bytes_planned: u64,
    pub bytes_freed: u64,
    pub bytes_measured: Option<i64>,
    pub items_done: u64,
    pub items_partial: u64,
    pub items_skipped: u64,
    pub items_refused: u64,
    pub items_failed: u64,
    pub items_cancelled: u64,
    pub kind: String,
    /// Arrêt avant les actions (prérequis obligatoire non rempli).
    pub aborted: Option<String>,
    /// « Continuer sans point de restauration » confirmé.
    pub without_restore_point: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ItemRow {
    pub id: i64,
    pub action_id: i64,
    pub path: String,
    pub rule_id: Option<String>,
    pub directory: bool,
    pub size: u64,
    pub method: String,
    pub result: String,
    pub reason: Option<String>,
    #[serde(skip)]
    pub volume_serial: Option<u64>,
    #[serde(skip)]
    pub file_id: Option<u128>,
    #[serde(skip)]
    pub recycle_item: Option<String>,
    pub restored_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommandRow {
    pub role: String,
    pub program: Option<String>,
    pub args: Vec<String>,
    pub status: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    /// Fichier de sortie rattaché (journal de DISM) et sa fin, pour l'affichage.
    pub log_path: Option<String>,
    pub log_tail: Option<String>,
}

pub fn status_name(s: &CommandStatus) -> &'static str {
    match s {
        CommandStatus::Succeeded => "succeeded",
        CommandStatus::Failed(_) => "failed",
        CommandStatus::TimedOut => "timedOut",
        CommandStatus::NotFound => "notFound",
        CommandStatus::NeedsElevation => "needsElevation",
        CommandStatus::Declined => "declined",
        CommandStatus::Cancelled => "cancelled",
        CommandStatus::Error(_) => "error",
    }
}

pub fn outcome_name(o: &ItemOutcome) -> &'static str {
    match o {
        ItemOutcome::Done => "done",
        ItemOutcome::Partial => "partial",
        ItemOutcome::Skipped(_) => "skipped",
        ItemOutcome::Refused(_) => "refused",
        ItemOutcome::Failed(_) => "failed",
        ItemOutcome::Cancelled => "cancelled",
    }
}

pub fn outcome_reason(o: &ItemOutcome) -> Option<String> {
    match o {
        ItemOutcome::Skipped(r) | ItemOutcome::Failed(r) => Some(r.clone()),
        ItemOutcome::Refused(r) => Some(r.to_string()),
        ItemOutcome::Done | ItemOutcome::Partial | ItemOutcome::Cancelled => None,
    }
}

/// Bilan global : tout fait, en partie, rien, ou annulé.
pub fn report_status(r: &Report) -> &'static str {
    if r.cancelled {
        return "cancelled";
    }
    // Un point de restauration déjà récent compte comme rempli (`satisfied`).
    let commands_ok = r.commands.iter().filter(|c| c.role == CommandRole::Action && c.satisfied()).count();
    let all_items = r.items.iter().all(|i| i.outcome == ItemOutcome::Done);
    let all_commands = r.commands.iter().all(|c| c.satisfied());
    if r.aborted.is_some() {
        return if r.freed_estimate() > 0 || commands_ok > 0 { "partial" } else { "failed" };
    }
    if all_items && all_commands {
        "ok"
    } else if r.freed_estimate() > 0 || commands_ok > 0 {
        "partial"
    } else {
        "failed"
    }
}

fn hex128(v: u128) -> String {
    format!("{v:032x}")
}

impl Journal {
    pub fn open(path: &Path) -> Result<Journal, JournalError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Journal::init(conn)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Journal, JournalError> {
        Journal::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Journal, JournalError> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        let version: Option<String> = conn.query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0)).optional()?;
        let version = match version.and_then(|v| v.parse::<i64>().ok()) {
            Some(v) if v > SCHEMA_VERSION => return Err(JournalError::Newer(v)),
            Some(v) => v,
            None => {
                conn.execute("INSERT INTO meta (key, value) VALUES ('schema_version', '1')", [])?;
                1
            }
        };
        for (v, sql) in MIGRATIONS {
            if version < v {
                let tx = conn.unchecked_transaction()?;
                tx.execute_batch(sql)?;
                tx.execute("UPDATE meta SET value = ?1 WHERE key = 'schema_version'", params![v.to_string()])?;
                tx.commit()?;
            }
        }
        Ok(Journal { conn })
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    pub(crate) fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Enregistre une exécution et tout son détail, en une transaction.
    pub fn record(&mut self, meta: &ActionMeta<'_>, report: &Report) -> Result<i64, JournalError> {
        let count = |f: fn(&ItemOutcome) -> bool| report.items.iter().filter(|i| f(&i.outcome)).count() as i64;
        let method = match report.mode {
            DeleteMode::Trash => "trash",
            DeleteMode::Permanent => "permanent",
        };
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO actions (started_at, finished_at, title, method, plan_hash, status, bytes_planned, bytes_freed, bytes_measured,
               items_done, items_partial, items_skipped, items_refused, items_failed, items_cancelled, script, kind, aborted, without_restore_point,
               volume_key)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                meta.started_at,
                meta.finished_at,
                meta.title,
                method,
                safety::plan::hex(&report.fingerprint),
                report_status(report),
                meta.planned_bytes as i64,
                report.freed_estimate() as i64,
                report.measured_gain(),
                count(|o| *o == ItemOutcome::Done),
                count(|o| *o == ItemOutcome::Partial),
                count(|o| matches!(o, ItemOutcome::Skipped(_))),
                count(|o| matches!(o, ItemOutcome::Refused(_))),
                count(|o| matches!(o, ItemOutcome::Failed(_))),
                count(|o| *o == ItemOutcome::Cancelled),
                meta.script,
                meta.kind,
                report.aborted,
                meta.without_restore_point,
                meta.volume_key,
            ],
        )?;
        let action = tx.last_insert_rowid();
        {
            let mut item = tx.prepare(
                "INSERT INTO action_items (action_id, path, rule_id, directory, size, method, result, reason, volume_serial, file_id, recycle_item)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            for i in &report.items {
                let rule = match &i.authority {
                    safety::allow::Authority::Rule(id) => Some(id.clone()),
                    safety::allow::Authority::Selection(_) | safety::allow::Authority::Duplicates(_) => None,
                };
                if i.units.is_empty() {
                    let result = match &i.outcome {
                        ItemOutcome::Refused(_) => "refused",
                        ItemOutcome::Cancelled => "cancelled",
                        ItemOutcome::Failed(_) => "failed",
                        _ => "skipped",
                    };
                    item.execute(params![action, i.path, rule, false, i.planned_bytes as i64, method, result, outcome_reason(&i.outcome), None::<i64>, None::<String>, None::<String>])?;
                }
                for u in &i.units {
                    let (result, reason, bin) = match &u.result {
                        Ok(r) => ("done", None, r.bin_path.clone()),
                        Err(e) => ("failed", Some(e.to_string()), None),
                    };
                    item.execute(params![
                        action,
                        u.path.display_string(),
                        rule,
                        u.directory,
                        u.bytes as i64,
                        method,
                        result,
                        reason,
                        u.file_id.volume_serial as i64,
                        hex128(u.file_id.id),
                        bin
                    ])?;
                }
                for issue in &i.issues {
                    item.execute(params![action, issue.path, rule, false, 0i64, method, "left", issue.reason.to_string(), None::<i64>, None::<String>, None::<String>])?;
                }
            }
            let mut cmd = tx.prepare(
                "INSERT INTO action_commands (action_id, role, program, args, status, exit_code, stdout, stderr, duration_ms, log_path)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            for c in &report.commands {
                let role = match c.role {
                    CommandRole::Prerequisite => "prerequisite",
                    CommandRole::Action => "action",
                };
                let code = match c.status {
                    CommandStatus::Failed(code) => Some(code),
                    CommandStatus::Succeeded => Some(0),
                    _ => None,
                };
                let mut stderr = c.stderr.clone();
                if let CommandStatus::Error(e) = &c.status {
                    stderr.push_str(e);
                }
                let args = serde_json::to_string(&c.args).unwrap_or_else(|_| "[]".into());
                cmd.execute(params![action, role, c.program, args, status_name(&c.status), code, c.stdout, stderr, c.duration_ms as i64, c.log])?;
            }
        }
        tx.commit()?;
        Ok(action)
    }

    pub fn actions(&self, limit: u32) -> Result<Vec<ActionRow>, JournalError> {
        let mut st = self.conn.prepare(
            "SELECT id, started_at, finished_at, title, method, status, bytes_planned, bytes_freed, bytes_measured,
               items_done, items_partial, items_skipped, items_refused, items_failed, items_cancelled, kind, aborted, without_restore_point
             FROM actions ORDER BY started_at DESC, id DESC LIMIT ?1",
        )?;
        let rows = st.query_map(params![limit], |r| {
            Ok(ActionRow {
                id: r.get(0)?,
                started_at: r.get(1)?,
                finished_at: r.get(2)?,
                title: r.get(3)?,
                method: r.get(4)?,
                status: r.get(5)?,
                bytes_planned: r.get::<_, i64>(6)? as u64,
                bytes_freed: r.get::<_, i64>(7)? as u64,
                bytes_measured: r.get(8)?,
                items_done: r.get::<_, i64>(9)? as u64,
                items_partial: r.get::<_, i64>(10)? as u64,
                items_skipped: r.get::<_, i64>(11)? as u64,
                items_refused: r.get::<_, i64>(12)? as u64,
                items_failed: r.get::<_, i64>(13)? as u64,
                items_cancelled: r.get::<_, i64>(14)? as u64,
                kind: r.get(15)?,
                aborted: r.get(16)?,
                without_restore_point: r.get(17)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn script(&self, action: i64) -> Result<Option<String>, JournalError> {
        Ok(self.conn.query_row("SELECT script FROM actions WHERE id = ?1", params![action], |r| r.get(0)).optional()?)
    }

    fn item_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<ItemRow> {
        Ok(ItemRow {
            id: r.get(0)?,
            action_id: r.get(1)?,
            path: r.get(2)?,
            rule_id: r.get(3)?,
            directory: r.get(4)?,
            size: r.get::<_, i64>(5)? as u64,
            method: r.get(6)?,
            result: r.get(7)?,
            reason: r.get(8)?,
            volume_serial: r.get::<_, Option<i64>>(9)?.map(|v| v as u64),
            file_id: r.get::<_, Option<String>>(10)?.and_then(|h| u128::from_str_radix(&h, 16).ok()),
            recycle_item: r.get(11)?,
            restored_at: r.get(12)?,
        })
    }

    const ITEM_COLUMNS: &'static str =
        "id, action_id, path, rule_id, directory, size, method, result, reason, volume_serial, file_id, recycle_item, restored_at";

    pub fn items(&self, action: i64, limit: u32) -> Result<Vec<ItemRow>, JournalError> {
        let sql = format!("SELECT {} FROM action_items WHERE action_id = ?1 ORDER BY id LIMIT ?2", Self::ITEM_COLUMNS);
        let mut st = self.conn.prepare(&sql)?;
        let rows = st.query_map(params![action, limit], Self::item_from)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn item(&self, id: i64) -> Result<Option<ItemRow>, JournalError> {
        let sql = format!("SELECT {} FROM action_items WHERE id = ?1", Self::ITEM_COLUMNS);
        Ok(self.conn.query_row(&sql, params![id], Self::item_from).optional()?)
    }

    pub fn commands(&self, action: i64) -> Result<Vec<CommandRow>, JournalError> {
        let mut st = self.conn.prepare(
            "SELECT role, program, args, status, exit_code, stdout, stderr, duration_ms, log_path FROM action_commands WHERE action_id = ?1 ORDER BY id",
        )?;
        let rows = st.query_map(params![action], |r| {
            Ok(CommandRow {
                role: r.get(0)?,
                program: r.get(1)?,
                args: serde_json::from_str(&r.get::<_, String>(2)?).unwrap_or_default(),
                status: r.get(3)?,
                exit_code: r.get(4)?,
                stdout: r.get(5)?,
                stderr: r.get(6)?,
                duration_ms: r.get::<_, i64>(7)? as u64,
                log_path: r.get(8)?,
                log_tail: None,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn mark_restored(&self, item: i64, at: i64) -> Result<(), JournalError> {
        self.conn.execute("UPDATE action_items SET restored_at = ?2 WHERE id = ?1", params![item, at])?;
        Ok(())
    }

    /// Empreintes connues d'un volume (doublons).
    pub fn hash_cache(&self, volume: u64) -> Result<HashMap<CacheKey, CacheEntry>, JournalError> {
        let mut st = self.conn.prepare("SELECT file_id, size, modified, edges, full FROM hash_cache WHERE volume_serial = ?1")?;
        let rows = st.query_map(params![volume as i64], |r| {
            let digest = |v: Option<Vec<u8>>| v.and_then(|b| <[u8; 32]>::try_from(b).ok());
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                digest(r.get(3)?),
                digest(r.get(4)?),
            ))
        })?;
        let mut map = HashMap::new();
        for row in rows {
            let (fid, size, modified, edges, full) = row?;
            let Ok(file_id) = u128::from_str_radix(&fid, 16) else { continue };
            map.insert(CacheKey { volume, file_id, size: size as u64, modified }, CacheEntry { edges, full });
        }
        Ok(map)
    }

    /// Enregistre des empreintes : un fichier n'a qu'un état en cache (l'ancien est remplacé),
    /// et ce qui n'a pas servi depuis 6 mois est oublié.
    pub fn store_hashes(&mut self, entries: &[(CacheKey, CacheEntry)], now_ms: i64) -> Result<(), JournalError> {
        let tx = self.conn.transaction()?;
        {
            let mut del = tx.prepare("DELETE FROM hash_cache WHERE volume_serial = ?1 AND file_id = ?2 AND (size <> ?3 OR modified <> ?4)")?;
            let mut put = tx.prepare(
                "INSERT INTO hash_cache (volume_serial, file_id, size, modified, edges, full, used_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT (volume_serial, file_id, size, modified)
                 DO UPDATE SET edges = COALESCE(excluded.edges, edges), full = COALESCE(excluded.full, full), used_at = excluded.used_at",
            )?;
            for (k, v) in entries {
                let fid = hex128(k.file_id);
                del.execute(params![k.volume as i64, fid, k.size as i64, k.modified])?;
                put.execute(params![k.volume as i64, fid, k.size as i64, k.modified, v.edges.map(|h| h.to_vec()), v.full.map(|h| h.to_vec()), now_ms])?;
            }
            tx.execute("DELETE FROM hash_cache WHERE used_at < ?1", params![now_ms - HASH_CACHE_TTL_MS])?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safety::allow::Authority;
    use safety::check::Change;
    use safety::commands::OfficialCommand;
    use safety::exec::{CommandRun, Issue, IssueReason, ItemReport, Recycled, RemoveError, UnitReport, VolumeSpace};
    use safety::path::WinPath;
    use safety::probe::FileId;

    fn unit(path: &str, bytes: u64, result: Result<Recycled, RemoveError>) -> UnitReport {
        UnitReport { path: WinPath::parse(path).unwrap(), bytes, directory: false, file_id: FileId { volume_serial: 7, id: 0xABCDEF }, result }
    }

    fn report() -> Report {
        Report {
            mode: DeleteMode::Trash,
            fingerprint: [0xAB; 32],
            commands: vec![
                CommandRun { program: Some("C:\\Program Files\\nodejs\\node.exe".into()), stdout: "ok".into(), ..CommandRun::new(&OfficialCommand::NpmCacheClean, CommandStatus::Succeeded) },
                CommandRun::new(&OfficialCommand::DismComponentCleanup(None), CommandStatus::NeedsElevation),
            ],
            items: vec![
                ItemReport {
                    path: "C:\\Users\\A\\cache\\sous".into(),
                    authority: Authority::Rule("cache".into()),
                    planned_bytes: 150,
                    outcome: ItemOutcome::Partial,
                    units: vec![
                        unit("C:\\Users\\A\\cache\\sous\\a.bin", 100, Ok(Recycled { bin_path: Some("C:\\$Recycle.Bin\\S-1\\$RAB12.bin".into()) })),
                        unit("C:\\Users\\A\\cache\\sous\\c.bin", 10, Err(RemoveError::Locked)),
                    ],
                    issues: vec![Issue { path: "C:\\Users\\A\\cache\\sous\\b.bin".into(), reason: IssueReason::Changed(Change::Size) }],
                },
                ItemReport {
                    path: "C:\\Users\\A\\cache\\x".into(),
                    authority: Authority::Rule("cache".into()),
                    planned_bytes: 5,
                    outcome: ItemOutcome::Skipped("l'élément a changé depuis l'analyse".into()),
                    units: Vec::new(),
                    issues: Vec::new(),
                },
            ],
            volumes: vec![VolumeSpace { volume: "C:\\".into(), before: Some(1000), after: Some(1120) }],
            cancelled: false,
            aborted: None,
        }
    }

    #[test]
    fn journal_complet_d_une_execution() {
        let mut j = Journal::in_memory().unwrap();
        let meta = ActionMeta { kind: "recos", without_restore_point: false, started_at: 10, finished_at: 20, title: "Nettoyage", planned_bytes: 155, script: "# Lumen · script", volume_key: None };
        let id = j.record(&meta, &report()).unwrap();
        let actions = j.actions(10).unwrap();
        assert_eq!(actions.len(), 1);
        let a = &actions[0];
        assert_eq!((a.id, a.status.as_str(), a.method.as_str()), (id, "partial", "trash"));
        assert_eq!((a.bytes_planned, a.bytes_freed, a.bytes_measured), (155, 100, Some(120)));
        assert_eq!((a.items_partial, a.items_skipped), (1, 1));
        assert_eq!(j.script(id).unwrap().as_deref(), Some("# Lumen · script"));
        assert_eq!((a.kind.as_str(), a.aborted.as_deref()), ("recos", None));

        let items = j.items(id, 100).unwrap();
        let results: Vec<(&str, &str)> = items.iter().map(|i| (i.path.as_str(), i.result.as_str())).collect();
        assert_eq!(
            results,
            [
                ("C:\\Users\\A\\cache\\sous\\a.bin", "done"),
                ("C:\\Users\\A\\cache\\sous\\c.bin", "failed"),
                ("C:\\Users\\A\\cache\\sous\\b.bin", "left"),
                ("C:\\Users\\A\\cache\\x", "skipped"),
            ]
        );
        let done = &items[0];
        assert_eq!(done.recycle_item.as_deref(), Some("C:\\$Recycle.Bin\\S-1\\$RAB12.bin"));
        assert_eq!((done.volume_serial, done.file_id, done.size), (Some(7), Some(0xABCDEF), 100));
        assert_eq!(items[1].reason.as_deref(), Some("ouvert par un autre programme"));
        assert_eq!(items[2].rule_id.as_deref(), Some("cache"));

        let commands = j.commands(id).unwrap();
        assert_eq!(commands[0].program.as_deref(), Some("C:\\Program Files\\nodejs\\node.exe"));
        assert_eq!(commands[0].args, ["cache", "clean", "--force"]);
        assert_eq!((commands[0].status.as_str(), commands[0].exit_code), ("succeeded", Some(0)));
        assert_eq!(commands[1].status, "needsElevation");

        j.mark_restored(done.id, 99).unwrap();
        assert_eq!(j.item(done.id).unwrap().unwrap().restored_at, Some(99));
        assert!(j.item(12345).unwrap().is_none());
    }

    #[test]
    fn bilans() {
        let mut r = report();
        assert_eq!(report_status(&r), "partial");
        r.cancelled = true;
        assert_eq!(report_status(&r), "cancelled");
        let mut r = report();
        r.items.clear();
        r.commands.truncate(1);
        assert_eq!(report_status(&r), "ok");
        let mut r = report();
        r.items.remove(0);
        r.commands.clear();
        assert_eq!(report_status(&r), "failed");
        // Point de restauration déjà récent (code 3) : rempli ; actions système faites.
        let mut r = report();
        r.items.clear();
        r.commands = vec![
            CommandRun { role: CommandRole::Prerequisite, ..CommandRun::new(&OfficialCommand::RestorePointCreate, CommandStatus::Failed(3)) },
            CommandRun::new(&OfficialCommand::DismComponentCleanup(None), CommandStatus::Succeeded),
        ];
        assert_eq!(report_status(&r), "ok");
        // Point de restauration impossible : arrêt avant les actions, rien de fait.
        r.commands[0].status = CommandStatus::Failed(5);
        r.commands[1].status = CommandStatus::Cancelled;
        r.aborted = Some("point de restauration non créé".into());
        assert_eq!(report_status(&r), "failed");
    }

    #[test]
    fn cache_des_empreintes() {
        let mut j = Journal::in_memory().unwrap();
        let k = CacheKey { volume: 7, file_id: 0xABC, size: 100, modified: 5 };
        j.store_hashes(&[(k, CacheEntry { edges: Some([1; 32]), full: None })], 1_000).unwrap();
        j.store_hashes(&[(k, CacheEntry { edges: None, full: Some([2; 32]) })], 2_000).unwrap();
        let c = j.hash_cache(7).unwrap();
        assert_eq!(c.get(&k), Some(&CacheEntry { edges: Some([1; 32]), full: Some([2; 32]) }));
        assert!(j.hash_cache(8).unwrap().is_empty());
        // Fichier modifié : l'ancien état disparaît du cache.
        let k2 = CacheKey { modified: 6, ..k };
        j.store_hashes(&[(k2, CacheEntry { edges: Some([3; 32]), full: None })], 3_000).unwrap();
        let c = j.hash_cache(7).unwrap();
        assert_eq!((c.len(), c.contains_key(&k2)), (1, true));
        // Oublié après 6 mois sans servir.
        j.store_hashes(&[], 3_000 + HASH_CACHE_TTL_MS + 1).unwrap();
        assert!(j.hash_cache(7).unwrap().is_empty());
    }

    #[test]
    fn migration_du_schema_v1() {
        let dir = tempdir();
        let path = dir.join("v1.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute("INSERT INTO meta (key, value) VALUES ('schema_version', '1')", []).unwrap();
        conn.execute(
            "INSERT INTO actions (started_at, finished_at, title, method, plan_hash, status, bytes_planned, bytes_freed,
               items_done, items_partial, items_skipped, items_refused, items_failed, items_cancelled, script)
             VALUES (1, 2, 'Ancien', 'trash', 'ab', 'ok', 10, 10, 1, 0, 0, 0, 0, 0, '')",
            [],
        )
        .unwrap();
        drop(conn);
        let j = Journal::open(&path).unwrap();
        let a = j.actions(10).unwrap();
        assert_eq!((a[0].title.as_str(), a[0].kind.as_str(), a[0].aborted.as_deref(), a[0].without_restore_point), ("Ancien", "recos", None, false));
        let version: String = j.conn.query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0)).unwrap();
        assert_eq!(version, SCHEMA_VERSION.to_string());
        assert!(j.hash_cache(1).unwrap().is_empty());
        drop(j);
        // Réouverture : déjà en v2, rien à refaire.
        assert!(Journal::open(&path).is_ok());
    }

    #[test]
    fn version_plus_recente_refusee() {
        let dir = tempdir();
        let path = dir.join("lumen.db");
        drop(Journal::open(&path).unwrap());
        let conn = Connection::open(&path).unwrap();
        conn.execute("UPDATE meta SET value = '99' WHERE key = 'schema_version'", []).unwrap();
        drop(conn);
        assert!(matches!(Journal::open(&path), Err(JournalError::Newer(99))));
    }

    fn tempdir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("lumen-journal-{}-{}", std::process::id(), line!()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
}

#[cfg(test)]
mod v3_tests {
    use super::*;
    use safety::commands::{OfficialCommand, OutputFile};
    use safety::exec::CommandRun;
    use safety::path::WinPath;

    #[test]
    fn journal_de_dism_et_sans_point_de_restauration() {
        let dir = std::env::temp_dir().join(format!("lumen-journal-v3-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Base v2 existante (jalon 6 avant correction) : montée en v3 sans perte.
        let path = dir.join("v2.db");
        drop(std::fs::File::create(&path));
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute("INSERT INTO meta (key, value) VALUES ('schema_version', '1')", []).unwrap();
            conn.execute_batch(SCHEMA_V2).unwrap();
            conn.execute("UPDATE meta SET value = '2' WHERE key = 'schema_version'", []).unwrap();
        }
        let mut j = Journal::open(&path).unwrap();
        let log = dir.join("dism-20261021-120000.log");
        let lines: Vec<String> = (0..200).map(|i| format!("ligne {i}")).collect();
        std::fs::write(&log, lines.join("\r\n")).unwrap();
        let out = OutputFile::new(&WinPath::parse("C:\\Lumen\\logs").unwrap(), "dism-20261021-120000.log").unwrap();
        let mut run = CommandRun::new(&OfficialCommand::DismComponentCleanup(Some(out)), safety::exec::CommandStatus::Succeeded);
        run.log = Some(log.to_string_lossy().into_owned());
        let report = Report {
            mode: DeleteMode::Trash,
            fingerprint: [1; 32],
            commands: vec![run],
            items: Vec::new(),
            volumes: Vec::new(),
            cancelled: false,
            aborted: None,
        };
        let meta = ActionMeta { kind: "system", without_restore_point: true, started_at: 1, finished_at: 2, title: "Gouffres système", planned_bytes: 0, script: "", volume_key: None };
        let id = j.record(&meta, &report).unwrap();
        let a = &j.actions(1).unwrap()[0];
        assert_eq!((a.kind.as_str(), a.without_restore_point), ("system", true));
        let c = &j.commands(id).unwrap()[0];
        assert_eq!(c.log_path.as_deref(), Some(log.to_string_lossy().as_ref()));
        let tail = log_tail(c.log_path.as_deref().unwrap()).unwrap();
        assert!(tail.ends_with("ligne 199") && tail.starts_with("ligne 120"), "{tail}");
        // Journal de DISM en UTF-16 (avec BOM).
        let utf16: Vec<u8> = "\u{feff}Résultat : terminé\r\nfin".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let log16 = dir.join("dism-utf16.log");
        std::fs::write(&log16, utf16).unwrap();
        assert_eq!(log_tail(log16.to_str().unwrap()).unwrap(), "Résultat : terminé\nfin");
        assert!(log_tail("Z:\\absent.log").is_none());
    }
}

#[cfg(test)]
mod v4_tests {
    use super::*;

    fn tables(conn: &Connection) -> Vec<String> {
        conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    }

    fn version(conn: &Connection) -> String {
        conn.query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn base_vide_jusqu_a_la_derniere_version() {
        let j = Journal::in_memory().unwrap();
        assert_eq!(version(j.conn()), SCHEMA_VERSION.to_string());
        assert_eq!(
            tables(j.conn()),
            ["action_commands", "action_items", "actions", "hash_cache", "meta", "snapshot_paths", "snapshot_sizes", "snapshots", "volumes"]
        );
        // Les colonnes ajoutées par les migrations sont là.
        j.conn().execute("UPDATE actions SET volume_key = NULL, without_restore_point = 0, kind = 'recos', aborted = NULL", []).unwrap();
        j.conn().execute("UPDATE action_commands SET log_path = NULL", []).unwrap();
    }

    #[test]
    fn migration_depuis_la_version_precedente() {
        let dir = std::env::temp_dir().join(format!("lumen-journal-v4-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v3.db");
        let _ = std::fs::File::create(&path);
        {
            // Base du jalon 6 (v3) avec un nettoyage et une empreinte en cache.
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute("INSERT INTO meta (key, value) VALUES ('schema_version', '1')", []).unwrap();
            conn.execute_batch(SCHEMA_V2).unwrap();
            conn.execute_batch(SCHEMA_V3).unwrap();
            conn.execute("UPDATE meta SET value = '3' WHERE key = 'schema_version'", []).unwrap();
            conn.execute(
                "INSERT INTO actions (started_at, finished_at, title, method, plan_hash, status, bytes_planned, bytes_freed,
                   items_done, items_partial, items_skipped, items_refused, items_failed, items_cancelled, script, kind, without_restore_point)
                 VALUES (1, 2, 'Jalon 6', 'trash', 'ab', 'ok', 10, 10, 1, 0, 0, 0, 0, 0, '', 'duplicates', 0)",
                [],
            )
            .unwrap();
            conn.execute("INSERT INTO hash_cache (volume_serial, file_id, size, modified, used_at) VALUES (1, 'f', 2, 3, 4)", []).unwrap();
            assert!(!tables(&conn).contains(&"snapshots".to_owned()));
        }
        let j = Journal::open(&path).unwrap();
        assert_eq!(version(j.conn()), "4");
        let a = j.actions(10).unwrap();
        assert_eq!((a.len(), a[0].title.as_str(), a[0].kind.as_str()), (1, "Jalon 6", "duplicates"));
        let key: Option<String> = j.conn().query_row("SELECT volume_key FROM actions", [], |r| r.get(0)).unwrap();
        assert_eq!(key, None);
        assert_eq!(j.hash_cache(1).unwrap().len(), 1);
        assert!(tables(j.conn()).contains(&"snapshots".to_owned()));
        drop(j);
        // Réouverture : déjà en v4, aucune migration rejouée (un ALTER rejoué échouerait).
        let j = Journal::open(&path).unwrap();
        assert_eq!(version(j.conn()), "4");
    }
}
