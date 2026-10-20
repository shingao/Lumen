# Lumen

Appli Windows 11 qui explique pourquoi le disque est plein et aide à le libérer **sans risque** : treemap lisible, recommandations classées « sans risque » / « à vérifier », scripts PowerShell en simulation par défaut, rien n'est supprimé sans accord et tout passe par la Corbeille.

- Maquette : `design/lumen-mockup.html`
- Architecture : `docs/PLAN.md`
- Avancement : `PROGRESS.md`
- Conventions et commandes : `CLAUDE.md`

```bash
npm install
npm run tauri dev   # appli Windows
npm run dev         # interface seule dans le navigateur (données de démonstration)
npm run check       # typecheck, lint, tests
```

100 % local : aucun accès réseau, aucune télémétrie.
