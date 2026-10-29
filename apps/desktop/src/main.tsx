import "@cers/ui/tokens.css";
import "./theme/app.css";

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./app/App";

const container = document.getElementById("root");
if (!container) {
  throw new Error("找不到应用挂载点 #root");
}

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
