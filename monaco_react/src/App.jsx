
import React, { useEffect, useRef, useState } from "react";
import "./App.css";
import ButtonsDisplay from "./Buttons";
import MenuBarDisplay from "./MenuBar";
import ActiveTabDisplay from "./ActiveTab";
import * as monaco from "monaco-editor";
import { invoke } from "@tauri-apps/api/tauri";

function App() {
  const BackendEnum = {
    SARC: 'SARC',
    YAML: 'YAML',
    Options: 'Options',
  };
  const [activeTab, setActiveTab] = useState('SARC'); // Adjust this initial value as needed
  const editorContainerRef = useRef(null);
  const editorRef = useRef(null);
  const [editorContent, setEditorContent] = useState("");
  const [greetMsg, setGreetMsg] = useState("");

  useEffect(() => {
    if (activeTab === BackendEnum.YAML) {
      editorRef.current = monaco.editor.create(editorContainerRef.current, {
        value: "// Type your name here\nconsole.log('Hello, world!')",
        language: "yaml",
        theme: "vs-dark",
      });

      function updateEditorSize() {
        const { width, height } = editorContainerRef.current.getBoundingClientRect();
        editorRef.current.layout({ width, height });
      }

      window.addEventListener("resize", updateEditorSize);

      // Cleanup
      return () => {
        window.removeEventListener("resize", updateEditorSize);
        if (editorRef.current) {
          editorRef.current.dispose();
          editorRef.current = null;
        }
      };
    }
  }, [activeTab]);
  //Variables

//Functions
  async function greet() {
    const name = editorRef.current.getValue();
    setGreetMsg(await invoke("greet", { name }));
  }

  const sendTextToRust = async () => {
    const editorText = editorRef.current.getValue(); // Get current text from Monaco Editor
    try {
      await invoke('receive_text_from_editor', { text: editorText }); // Invoke the Rust command
      console.log('Text sent to Rust backend successfully.');
    } catch (error) {
      console.error('Failed to send text to Rust backend:', error);
    }
  };

  const updateEditorSize = () => { //make monaco editor scale with the window
    if (editorContainerRef.current && editorRef.current) {
      const { width, height } = editorContainerRef.current.getBoundingClientRect();
      editorRef.current.layout({ width, height });
    }
  };




  return (
    <div>
      <MenuBarDisplay />
      <ActiveTabDisplay activeTab={activeTab} setActiveTab={setActiveTab} />
      <ButtonsDisplay />
      {activeTab === 'YAML' && <div ref={editorContainerRef} className="code_editor"></div>}
      <div className="statusbar">Status Bar</div>
    </div>
  );
}

export default App;
