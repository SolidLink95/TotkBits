import { invoke } from '@tauri-apps/api/tauri';
import * as monaco from "monaco-editor";
import { OpenFileFromPath } from './ButtonClicks';


const InitializeEditor = (props) => {
  let startupData = { argv1: '', fontSize: 14, theme: 'vs-dark', minimap: false };
  const {
    editorRef,
    editorContainerRef,
    editorValue,
    lang,
    setStatusText,
    setActiveTab,
    setLabelTextDisplay,
    setpaths,
    updateEditorContent,
  } = props;

  console.log("Initializing Monaco editor");
  
  editorRef.current = monaco.editor.create(editorContainerRef.current, {
    value: editorValue,
    language: lang,
    theme: startupData["theme"],
    minimap: { enabled: startupData["minimap"] },
    wordWrap: 'on',
    fontSize: startupData["fontSize"], // Initial fontSize setup
  });

  try {
    invoke('get_startup_data')
      .then((data) => {
        const arg = data["argv1"] || startupData["argv1"];
        const fontSize = data["fontSize"] || startupData["fontSize"];
        const theme = data["theme"] || startupData["theme"];
        const minimap = data["theme"] || startupData["minimap"];
        editorRef.current.updateOptions({ fontSize: fontSize });
        editorRef.current.updateOptions({ theme: theme });
        editorRef.current.updateOptions({ minimap: { enabled: minimap } });
        console.log("Startup data:", data);
        
        if (arg) {
          console.log('Received command-line argument:', arg);
          OpenFileFromPath(arg, setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);
        } else {
          console.log('No command-line argument provided.');
        }
      })
      .catch((error) => {
        console.error('Error fetching command-line argument:', error);
      });
  } catch (error) {
    console.error('Failed to fetch command-line argument:', error);
  }
};

export default InitializeEditor;
