import "./styles.css";

import { initTheme, Toaster, TooltipProvider } from "@vitavision/ui";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { ApiContext, loadApi } from "./api";
import { App } from "./app/App";

initTheme();

const root = document.getElementById("root");
if (!root) throw new Error("index.html has no #root");

void loadApi().then((api) => {
  createRoot(root).render(
    <StrictMode>
      <ApiContext value={api}>
        <TooltipProvider>
          <App />
          <Toaster />
        </TooltipProvider>
      </ApiContext>
    </StrictMode>,
  );
});
