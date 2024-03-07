
import React, { useEffect, useRef, useState } from "react";
import "./App.css";
import DirectoryTree from "./PathList";
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
  const [selectedPath, setSelectedPath] = useState('');
  const paths = [
    "folder1/subfolder1/file1.txt",
    "folder1/subfolder1/file11.txt",
    "folder1/subfolder2/file2.txt",
    "folder2/subfolder1/file3.txt",
    "folder3/file4.txt",
  ];

  const added_paths = [
    "folder1/subfolder1/file11.txt",
  ];
  
  const modded_paths = [
    "folder3/file4.txt",
  ];

  const updateEditorContent = (content) => {
    if (editorRef.current) {
        editorRef.current.setValue(content);
        console.log(content);
    }
  };


  const handleNodeSelect = (path) => {
    setSelectedPath(path);
    console.log(`Selected Node Path in App: ${path}`);
    // Here you can use selectedPath for any other logic in App.jsx
  };


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
      <ButtonsDisplay updateEditorContent={updateEditorContent} />
      {activeTab === 'SARC' && <DirectoryTree onNodeSelect={handleNodeSelect} paths={paths} added_paths={added_paths} modded_paths={modded_paths} />}
      {activeTab === 'YAML' && <div ref={editorContainerRef} className="code_editor"></div>}
      <div className="statusbar">Status Bar</div>
    </div>
  );
}

export default App;
