//! Corbeille via `IFileOperation` (annulable depuis l'Explorateur), un élément par opération
//! pour un bilan exact, avec un « sink » qui note où Windows a rangé l'élément dans la Corbeille
//! (pour le restaurer depuis le journal).

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

use windows::core::{implement, Ref, HRESULT, HSTRING, PCWSTR};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::Shell::{
    FileOperation, IFileOperation, IFileOperationProgressSink, IFileOperationProgressSink_Impl, IShellItem,
    SHCreateItemFromParsingName, FOFX_EARLYFAILURE, FOFX_RECYCLEONDELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI,
    FOF_SILENT, FOF_WANTNUKEWARNING, SIGDN_FILESYSPATH,
};

use crate::bin::{recycle_allowed, RecycleBlock};
use crate::exec::Recycled;
use crate::path::WinPath;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecycleError {
    #[error("envoi à la Corbeille impossible : {0}")]
    Shell(String),
    #[error("opération annulée par Windows")]
    Aborted,
    #[error("le fil de la Corbeille s'est arrêté inopinément")]
    Thread,
    #[error("Corbeille indisponible : {0}")]
    NoRecycleBin(RecycleBlock),
    #[error("état du volume illisible : {0}")]
    Volume(String),
}

impl From<windows::core::Error> for RecycleError {
    fn from(e: windows::core::Error) -> Self {
        RecycleError::Shell(e.message())
    }
}

struct ComApartment;

impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY : appairé avec un CoInitializeEx réussi sur ce fil.
        unsafe { CoUninitialize() };
    }
}

/// Résultat de PostDeleteItem : code de Windows et emplacement dans la Corbeille.
type Outcome = Rc<RefCell<Option<(HRESULT, Option<String>)>>>;

#[implement(IFileOperationProgressSink)]
struct Sink {
    outcome: Outcome,
}

fn file_system_path(item: &IShellItem) -> Option<String> {
    // SAFETY : interface valide ; la chaîne renvoyée est libérée par CoTaskMemFree.
    unsafe {
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const c_void));
        s
    }
}

#[allow(non_snake_case)]
impl IFileOperationProgressSink_Impl for Sink_Impl {
    fn StartOperations(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn FinishOperations(&self, _: HRESULT) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreRenameItem(&self, _: u32, _: Ref<IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostRenameItem(&self, _: u32, _: Ref<IShellItem>, _: &PCWSTR, _: HRESULT, _: Ref<IShellItem>) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreMoveItem(&self, _: u32, _: Ref<IShellItem>, _: Ref<IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostMoveItem(&self, _: u32, _: Ref<IShellItem>, _: Ref<IShellItem>, _: &PCWSTR, _: HRESULT, _: Ref<IShellItem>) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreCopyItem(&self, _: u32, _: Ref<IShellItem>, _: Ref<IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostCopyItem(&self, _: u32, _: Ref<IShellItem>, _: Ref<IShellItem>, _: &PCWSTR, _: HRESULT, _: Ref<IShellItem>) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreDeleteItem(&self, _: u32, _: Ref<IShellItem>) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostDeleteItem(&self, _: u32, _: Ref<IShellItem>, hr: HRESULT, created: Ref<IShellItem>) -> windows::core::Result<()> {
        let bin = created.ok().ok().and_then(file_system_path);
        *self.outcome.borrow_mut() = Some((hr, bin));
        Ok(())
    }
    fn PreNewItem(&self, _: u32, _: Ref<IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostNewItem(&self, _: u32, _: Ref<IShellItem>, _: &PCWSTR, _: &PCWSTR, _: u32, _: HRESULT, _: Ref<IShellItem>) -> windows::core::Result<()> {
        Ok(())
    }
    fn UpdateProgress(&self, _: u32, _: u32) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResetTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
}

/// Un élément, sur un fil déjà initialisé en STA.
fn recycle_on_this_thread(path: &str) -> Result<Recycled, RecycleError> {
    let outcome: Outcome = Rc::default();
    // SAFETY : interfaces COM utilisées sur le fil qui les a créées, libérées (Drop) avant
    // CoUninitialize grâce à l'ordre des variables de l'appelant.
    unsafe {
        let op: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL)?;
        // Corbeille obligatoire : FOF_WANTNUKEWARNING empêche toute suppression définitive silencieuse
        // (élément trop gros pour la Corbeille, volume sans Corbeille).
        op.SetOperationFlags(
            FOF_ALLOWUNDO | FOFX_RECYCLEONDELETE | FOF_WANTNUKEWARNING | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI | FOFX_EARLYFAILURE,
        )?;
        let sink: IFileOperationProgressSink = Sink { outcome: outcome.clone() }.into();
        let cookie = op.Advise(&sink)?;
        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(path), None)?;
        op.DeleteItem(&item, None)?;
        let performed = op.PerformOperations();
        let _ = op.Unadvise(cookie);
        performed?;
        if op.GetAnyOperationsAborted()?.as_bool() {
            return Err(RecycleError::Aborted);
        }
    }
    let result = outcome.borrow_mut().take();
    match result {
        Some((hr, bin_path)) if hr.is_ok() => Ok(Recycled { bin_path }),
        Some((hr, _)) => Err(RecycleError::Shell(windows::core::Error::from_hresult(hr).message())),
        None => Err(RecycleError::Shell("Windows n'a rien signalé pour cet élément".into())),
    }
}

/// Envoie à la Corbeille des éléments déjà recontrôlés par `safety`, un par un ; un résultat
/// par élément, dans l'ordre.
///
/// Deux protections contre une destruction silencieuse : le pré-contrôle explicite du volume
/// (type de lecteur, SUBST, Corbeille active, taille maximale), refait ici juste avant l'appel,
/// puis FOF_WANTNUKEWARNING, qui l'emporte sur FOF_NOCONFIRMATION si Windows devait malgré tout
/// supprimer définitivement. FOF_NOCONFIRMATION n'est jamais utilisé seul.
pub(crate) fn recycle_many(items: &[(WinPath, u64)]) -> Vec<Result<Recycled, RecycleError>> {
    let jobs: Vec<Result<String, RecycleError>> = items
        .iter()
        .map(|(path, bytes)| {
            let facts = super::volume_facts(path).map_err(|e| RecycleError::Volume(e.to_string()))?;
            recycle_allowed(&facts, *bytes).map_err(RecycleError::NoRecycleBin)?;
            Ok(path.display_string())
        })
        .collect();
    let n = jobs.len();
    // COM est initialisé sur un fil dédié pour ne jamais dépendre de l'appartement de l'appelant.
    std::thread::spawn(move || {
        // SAFETY : fil dédié, initialisé en STA puis libéré par ComApartment.
        if let Err(e) = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }.ok() {
            return jobs.into_iter().map(|_| Err(RecycleError::from(e.clone()))).collect();
        }
        let _apartment = ComApartment;
        jobs.into_iter().map(|job| job.and_then(|path| recycle_on_this_thread(&path))).collect::<Vec<_>>()
    })
    .join()
    .unwrap_or_else(|_| (0..n).map(|_| Err(RecycleError::Thread)).collect())
}

/// Un seul élément (tests).
#[cfg(test)]
pub(crate) fn recycle(path: &WinPath, bytes: u64) -> Result<Recycled, RecycleError> {
    recycle_many(&[(path.clone(), bytes)]).pop().unwrap_or(Err(RecycleError::Thread))
}
