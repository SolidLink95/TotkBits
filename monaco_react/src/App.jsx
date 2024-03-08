
import React, { useEffect, useRef, useState } from "react";
import "./App.css";
import DirectoryTree from "./PathList";
import ButtonsDisplay from "./Buttons";
import MenuBarDisplay from "./MenuBar";
import ActiveTabDisplay from "./ActiveTab";
import * as monaco from "monaco-editor";
import { invoke } from "@tauri-apps/api/tauri";
import { debounce } from "lodash"; // or any other method/utility to debounce



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
  const [statusText, setStatusText] = useState("");
  const [selectedPath, setSelectedPath] = useState({path: "", endian: ""});

  const sarcPaths = {
    "paths": [
      "folder1/subfolder1/file1.txt",
      "folder1/subfolder1/file11.txt",
      "folder1/subfolder2/file2.txt",
      "folder2/subfolder1/file3.txt",
      "folder3/file4.txt",
    ],
    "added_paths": [
      "folder1/subfolder1/file11.txt",
    ],
    "modded_paths": [
      "folder3/file4.txt",
    ]
  };
  const fetchStatusString = async () => {
    try {
      const statusText = await invoke('get_status_text'); // Match the command name
      setStatusText(statusText ? statusText : "Ready XXX"); // Set the status text (or handle it as needed
      console.log(statusText);
  } catch (e) {
      console.error('Failed to get status text', e);
  }
  }

  const updateEditorContent = (content) => {
    if (editorRef.current) {
        editorRef.current.setValue(content);
        console.log(content);
    }
  };


  const handleNodeSelect = (path, endian) => {
    setSelectedPath({path, endian});
    console.log(`Selected Node Path in App: ${path} endian: ${endian}`);
    // Here you can use selectedPath for any other logic in App.jsx
  };


  useEffect(() => {
    
    fetchStatusString();
    if (activeTab === BackendEnum.YAML) {
      editorRef.current = monaco.editor.create(editorContainerRef.current, {
        value: "// Type your name here\nconsole.log('Hello, world!')",
        language: "yaml",
        theme: "vs-dark",
      });

      const debouncedUpdateEditorSize = debounce(function updateEditorSize() {
        const { width, height } = editorContainerRef.current.getBoundingClientRect();
        editorRef.current.layout({ width, height });
      }, 100); // Adjust debounce timing as needed
    
      window.addEventListener("resize", debouncedUpdateEditorSize);
      // Cleanup
      return () => {
        window.removeEventListener("resize", debouncedUpdateEditorSize);
        if (editorRef.current) {
          editorRef.current.dispose();
          editorRef.current = null;
        }
      };
    }
  }, [activeTab]);
  //Variables

  //Functions

  const sendTextToRust = async () => {
    const editorText = editorRef.current.getValue(); // Get current text from Monaco Editor
    try {
      await invoke('receive_text_from_editor', { text: editorText }); // Invoke the Rust command
      console.log('Text sent to Rust backend successfully.');
    } catch (error) {
      console.error('Failed to send text to Rust backend:', error);
    }
  };
  const statusStyle = {
    color: statusText.toLowerCase().includes("error") ? 'red' : 'white',
  };



  return (
    <div>
      <MenuBarDisplay />
      <ActiveTabDisplay activeTab={activeTab} setActiveTab={setActiveTab} />
      <ButtonsDisplay updateEditorContent={updateEditorContent} />
      {activeTab === 'SARC' && <DirectoryTree onNodeSelect={handleNodeSelect}  sarcPaths={sarcPaths} />}
      {activeTab === 'YAML' && <div ref={editorContainerRef} className="code_editor"></div>}
      {/* <div className="statusbar" style={statusStyle}>Current path: "{selectedPath.path} {selectedPath.endian}"</div> */}
      <div className="statusbar" style={statusStyle}>{statusText}</div>
    </div>
  );
}

export default App;
