import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles/fonts.css";
import "./styles/tokens.css";
import "./styles/global.css";
import { App } from "./app/App";
import { initAppearance } from "./app/theme";
import { useStore } from "./app/store";
import { DEFAULT_CHECKED } from "./ipc/mock/data";

const root = document.getElementById("root");

void initAppearance().finally(() => {
  // Jalon 1 : sélection par défaut de la maquette (uniquement des éléments sans risque).
  useStore.getState().setChecked(DEFAULT_CHECKED);
  if (root) {
    createRoot(root).render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
  }
});
