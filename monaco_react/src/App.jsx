import { invoke } from "@tauri-apps/api/tauri";
import * as monaco from 'monaco-editor';
import { useEffect, useRef, useState } from "react";
import "./App.css";
import ButtonsDisplay from "./Buttons";

function App() {
  //Variables
  const [editorContent, setEditorContent] = useState("");
  const [greetMsg, setGreetMsg] = useState("");
  const editorContainerRef = useRef(null);
  const editorRef = useRef(null);
  const [showDropdown, setShowDropdown] = useState({ file: false, view: false, tools: false });
  const dropdownRefs = useRef({ file: null, view: null, tools: null });

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

  const toggleDropdown = (menu) => {//menu bar logic
    setShowDropdown(prevState => ({
      ...{ file: false, view: false, tools: false }, // Reset all to false
      [menu]: !prevState[menu] // Toggle the clicked one
    }));
  };

  const handleClickOutside = (event) => {
    // Close all dropdowns if the click is outside of any dropdown reference
    if (Object.values(dropdownRefs.current).every(ref => ref && !ref.contains(event.target))) {
      setShowDropdown({ file: false, view: false, tools: false });
    }
  };


  useEffect(() => {
    document.addEventListener('mousedown', handleClickOutside);

    monaco.editor.setTheme('vs-dark');
    editorRef.current = monaco.editor.create(editorContainerRef.current, {
      value: "// Type your name here\nconsole.log('Hello, world!')",
      language: 'javascript',
      theme: 'vs-dark'
    });
    window.addEventListener('resize', updateEditorSize);

    //Cleanup
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      window.removeEventListener('resize', updateEditorSize);
      if (editorRef.current) {
        editorRef.current.dispose();
      }
    };
  }, []);

  return (
    <div>
      <div className="menu-bar">
        {/* Each menu-item click handler is now just calling toggleDropdown */}
        <div className="menu-item" onClick={() => toggleDropdown('file')} ref={el => dropdownRefs.current.file = el}>File
          <div className="dropdown-content" style={{ display: showDropdown.file ? 'block' : 'none' }}>
            <a href="#">New</a>
            <a href="#">Open</a>
            <a href="#">Save</a>
          </div>
        </div>
        <div className="menu-item" onClick={() => toggleDropdown('view')} ref={el => dropdownRefs.current.view = el}>View
          <div className="dropdown-content" style={{ display: showDropdown.view ? 'block' : 'none' }}>
            <a href="#">Zoom In</a>
            <a href="#">Zoom Out</a>
            <a href="#">Reset Zoom</a>
          </div>
        </div>
        <div className="menu-item" onClick={() => toggleDropdown('tools')} ref={el => dropdownRefs.current.tools = el}>Tools
          <div className="dropdown-content" style={{ display: showDropdown.tools ? 'block' : 'none' }}>
            <a href="#">Options</a>
            <a href="#">Extensions</a>
          </div>
        </div>
        
      </div>
        {/* <div style={{}}>
          <button onClick={sendTextToRust}>Send to Rust</button>
        </div> */}
        <ButtonsDisplay />
      <div ref={editorContainerRef} className="code_editor" style={{ marginTop: "120px" }}></div>
      <div className="statusbar">
        Status Bar
      </div>
    </div>
  );
}

export default App;
