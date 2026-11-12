import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import CalendarWindow from "./CalendarWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ErrorBoundary } from "./ErrorBoundary";
import { applyDisplay } from "./display";

applyDisplay();

function windowLabel(): string {
  try { return getCurrentWindow().label; } catch { return "main"; }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      {windowLabel() === "calendar" ? <CalendarWindow /> : <App />}
    </ErrorBoundary>
  </React.StrictMode>,
);
