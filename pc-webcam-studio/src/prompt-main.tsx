import React from "react";
import ReactDOM from "react-dom/client";
import { UsbPromptApp } from "./UsbPromptApp";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <UsbPromptApp />
  </React.StrictMode>
);
