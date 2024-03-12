import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { EditorProvider } from './StateManager'; // Adjust the path as necessary
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root")).render(
  <React.StrictMode>
  <EditorProvider>
    <App />
  </EditorProvider>
  </React.StrictMode>,
);
