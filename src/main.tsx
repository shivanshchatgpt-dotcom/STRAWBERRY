import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/global.css";

// Theme bootstrap — dark by default, persisted choice wins.
document.documentElement.dataset.theme =
  (localStorage.getItem("strawberry-theme") as "dark" | "light") ?? "dark";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
