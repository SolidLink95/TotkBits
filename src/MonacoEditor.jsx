import { invoke } from '@tauri-apps/api/tauri';
import * as monaco from "monaco-editor";
import { OpenFileFromPath } from './ButtonClicks';

const defaultStartupData = { argv1: '', fontSize: 14, theme: 'vs-dark', minimap: false };

const InitializeEditor = (props) => {
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

  // Create the editor with default options
  editorRef.current = monaco.editor.create(editorContainerRef.current, {
    value: editorValue,
    language: lang,
    theme: defaultStartupData.theme,
    minimap: { enabled: defaultStartupData.minimap },
    wordWrap: 'on',
    fontSize: defaultStartupData.fontSize,
  });

  invoke('get_startup_data').then((data) => {
    // Use object spread to combine default settings with fetched data
    const effectiveData = { ...defaultStartupData, ...data };
    console.log("Startup data:", effectiveData);

    // Update all configurable options at once
    editorRef.current.updateOptions({
      fontSize: effectiveData.fontSize,
      theme: effectiveData.theme,
      minimap: { enabled: effectiveData.minimap }
    });

    if (effectiveData.argv1) {
      console.log('Received command-line argument:', effectiveData.argv1);
      OpenFileFromPath(effectiveData.argv1, setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);
    } else {
      console.log('No command-line argument provided.');
    }
  }).catch((error) => {
    console.error('Error fetching startup data:', error);
  });
};

export default InitializeEditor;
