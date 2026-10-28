import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles/fonts.css";
import "./styles/tokens.css";
import "./styles/global.css";
import { App } from "./app/App";
import { initScanning } from "./app/scanning";
import { initAppearance } from "./app/theme";
import { initOnboarding } from "./app/onboarding";
import { useStore } from "./app/store";
import { DEFAULT_CHECKED } from "./ipc/mock/data";

const root = document.getElementById("root");

void initAppearance().finally(() => {
  // Premier lancement : tutoriel, pendant que la première analyse tourne en arrière-plan.
  initOnboarding(useStore.getState().settings);
  // Jalon 1 : sélection par défaut de la maquette (uniquement des éléments sans risque).
  useStore.getState().setChecked(DEFAULT_CHECKED);
  // Analyse du dossier utilisateur au démarrage : parcours, jamais de demande UAC.
  void initScanning();
  if (root) {
    createRoot(root).render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
  }
});
