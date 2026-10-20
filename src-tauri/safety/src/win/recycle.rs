//! Corbeille via `IFileOperation` (annulable depuis l'Explorateur).
//!
//! Jamais appelée par l'interface au jalon 2 : `plan::execute` refuse toute exécution réelle.
//! Elle n'est utilisée que par les tests Windows, sur des fixtures temporaires.
// Branchée sur plan::execute au jalon 5 ; d'ici là, seuls les tests l'appellent.
#![cfg_attr(not(test), allow(dead_code))]

use windows::core::HSTRING;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::Shell::{
    FileOperation, IFileOperation, IShellItem, SHCreateItemFromParsingName, FOFX_EARLYFAILURE, FOFX_RECYCLEONDELETE,
    FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOF_WANTNUKEWARNING,
};

use crate::path::WinPath;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecycleError {
    #[error("envoi à la Corbeille impossible : {0}")]
    Shell(String),
    #[error("opération annulée par Windows")]
    Aborted,
    #[error("le fil de la Corbeille s'est arrêté inopinément")]
    Thread,
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

fn recycle_on_this_thread(path: &str) -> Result<(), RecycleError> {
    // SAFETY : fil dédié, initialisé en STA puis libéré par ComApartment ; les interfaces COM
    // sont libérées (Drop) avant CoUninitialize grâce à l'ordre des variables.
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).ok()?;
        let _apartment = ComApartment;
        let op: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL)?;
        // Corbeille obligatoire : FOF_WANTNUKEWARNING empêche toute suppression définitive silencieuse
        // (élément trop gros pour la Corbeille, volume sans Corbeille).
        op.SetOperationFlags(
            FOF_ALLOWUNDO | FOFX_RECYCLEONDELETE | FOF_WANTNUKEWARNING | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI | FOFX_EARLYFAILURE,
        )?;
        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(path), None)?;
        op.DeleteItem(&item, None)?;
        op.PerformOperations()?;
        if op.GetAnyOperationsAborted()?.as_bool() {
            return Err(RecycleError::Aborted);
        }
        Ok(())
    }
}

/// Envoie un élément déjà validé par `safety` à la Corbeille.
pub(crate) fn recycle(path: &WinPath) -> Result<(), RecycleError> {
    let display = path.display_string();
    // COM est initialisé sur un fil dédié pour ne jamais dépendre de l'appartement de l'appelant.
    std::thread::spawn(move || recycle_on_this_thread(&display)).join().map_err(|_| RecycleError::Thread)?
}
