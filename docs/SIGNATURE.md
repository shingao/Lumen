# Signature de Lumen (placeholder)

Les builds 0.9.x ne sont pas signés : Windows SmartScreen affiche « Éditeur inconnu » à
l'installation. Tout est prêt pour signer dès qu'un certificat existe, sans rien changer au code.

## Ce qui est signé

Tauri signe chaque binaire **avant** de l'empaqueter, par la commande `bundle.windows.signCommand` :

1. `lumen.exe` (l'appli et l'assistant MFT élevé, même exécutable) ;
2. le désinstalleur `uninstall.exe` ;
3. l'installeur `Lumen_<version>_x64-setup.exe`.

Signer après coup l'installeur seul ne suffirait pas : `lumen.exe` à l'intérieur resterait non
signé, et l'assistant MFT, lancé en administrateur, doit l'être.

## En CI (`.github/workflows/windows.yml`, job `build`)

Ajouter deux secrets au dépôt (Settings › Secrets and variables › Actions) :

| Secret | Contenu |
|---|---|
| `LUMEN_SIGN_CERT` | le certificat `.pfx`, en base64 (`[Convert]::ToBase64String([IO.File]::ReadAllBytes("lumen.pfx"))`) |
| `LUMEN_SIGN_PASSWORD` | son mot de passe |

Avec eux, l'étape « Certificat de signature » écrit le `.pfx` dans le dossier temporaire du runner
et passe à `tauri build` une configuration `signCommand` qui appelle `scripts/sign.ps1` (signtool,
SHA-256, horodatage RFC 3161, vérification). Le `.pfx` est supprimé en fin de job. Sans eux, le build
reste non signé et rien d'autre ne change.

## En local

```powershell
$env:LUMEN_SIGN_PFX = "C:\chemin\lumen.pfx"
$env:LUMEN_SIGN_PASSWORD = "…"
npx tauri build --config '{"bundle":{"windows":{"signCommand":{"cmd":"pwsh","args":["-NoProfile","-File","C:/…/Lumen/scripts/sign.ps1","%1"]}}}}'
```

## Certificat

- Certificat de signature de code (OV ou EV) au nom de l'éditeur ; un certificat EV sur jeton
  matériel ou un service de signature cloud (Azure Trusted Signing…) demande d'adapter
  `scripts/sign.ps1` (signtool `/dlib` ou `/sha1` au lieu de `/f`).
- `publisher` dans `src-tauri/tauri.conf.json` doit alors porter le nom exact du certificat.
- La v1.0 sera signée ; d'ici là, chaque build 0.9.x est publié non signé.
