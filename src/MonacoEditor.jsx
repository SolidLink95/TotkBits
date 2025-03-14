import { invoke } from '@tauri-apps/api/tauri';
import * as monaco from "monaco-editor";
import { OpenFileFromPath } from './ButtonClicks';


const InitializeEditor = (props) => {
  const {
    editorRef,
    editorContainerRef,
    editorValue,
    // lang,
    setStatusText,
    setActiveTab,
    setLabelTextDisplay,
    setpaths,
    updateEditorContent,
    settings,
    setSettings,
  } = props;

  console.log("Initializing Monaco editor");

  // Create the editor with default options
  editorRef.current = monaco.editor.create(editorContainerRef.current, {
    value: editorValue,
    language: settings.lang,
    theme: settings.theme,
    minimap: { enabled: settings.minimap },
    wordWrap: 'on',
    fontSize: settings.fontSize,
  });

  invoke('get_startup_data').then((data) => {
    // Use object spread to combine default settings with fetched data
    const updatedSettings = { ...settings, ...data };
    // setSettings(updatedSettings);  // Update state for future re-renders
    settings.argv1 = updatedSettings.argv1;  
    settings.fontSize = updatedSettings.fontSize;  
    settings.theme = updatedSettings.theme;  
    settings.minimap = updatedSettings.minimap;  
    settings.contextMenuFontSize = updatedSettings.contextMenuFontSize;  
    settings.zstd_msg = data.zstd_msg;  
    


    console.log("settings:", settings);
    console.log("received data:", data);

    // Update all configurable options at once
    editorRef.current.updateOptions({
      fontSize: settings.fontSize,
      theme: settings.theme,
      minimap: { enabled: settings.minimap }
    });

    if (settings.argv1) {
      console.log('Received command-line argument:', settings.argv1);
      OpenFileFromPath(settings.argv1, setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);
    } else {
      console.log('No command-line argument provided.');
    }
  }).catch((error) => {
    console.error('Error fetching startup data:', error);
  });
};

export default InitializeEditor;
