import { invoke } from "@tauri-apps/api/tauri";
import { useEffect, useRef, useState } from "react";
import "./App.css";
// Import the Monaco Editor package
import * as monaco from 'monaco-editor';

function App() {
  const [greetMsg, setGreetMsg] = useState("");
  const editorContainerRef = useRef(null); // Ref for the editor container div
  const editorRef = useRef(null); // Ref to store the editor instance

  async function greet() {
    const name = editorRef.current.getValue(); // Get the current value from the editor
    setGreetMsg(await invoke("greet", { name }));
  }

  // Function to adjust editor size
  const updateEditorSize = () => {
    if (editorContainerRef.current && editorRef.current) {
      const { width, height } = editorContainerRef.current.getBoundingClientRect();
      editorRef.current.layout({
        width,
        height
      });
    }
  };

  useEffect(() => {
    // Initialize Monaco Editor
    monaco.editor.setTheme('vs-dark');
    editorRef.current = monaco.editor.create(editorContainerRef.current, {
      value: "// Type your name here\nconsole.log('Hello, world!')",
      language: 'javascript',
      theme: 'vs-dark'
    });

    // Add window resize listener to update editor size
    window.addEventListener('resize', updateEditorSize);

    // Initial size adjustment
    //updateEditorSize();

    return () => {
      window.removeEventListener('resize', updateEditorSize);
      if (editorRef.current) {
        editorRef.current.dispose(); // Dispose the editor on component unmount
      }
    };
  }, []);

  return (
    // <div className="container">
    <div >
      {/* <h1>Welcome to Tauri!</h1>
      <button onClick={greet}>Greet</button> */}
      {/* Monaco Editor container with ref */}
      <div ref={editorContainerRef} class={"code_editor"}  style={{   marginTop: "120px"}}></div>
      {/* Status bar below the editor */}
      <div style={{ width: '100%', height: '20px', backgroundColor: '#333', color: '#fff', display: 'flex', alignItems: 'left', justifyContent: 'center' }}>
        Status Bar
      </div>
      {/* <p>{greetMsg}</p> */}
    </div>
  );
}

export default App;
