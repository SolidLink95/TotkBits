import { invoke } from '@tauri-apps/api/tauri'; // Import Tauri invoke method

export const useExitApp = async () => {
  console.log('Exiting the app');
  try {
    invoke('exit_app');
  } catch (error) {
    console.error(error);
  }
};

export async function openInternalSarcFile(selectedPath, setStatusText, setActiveTab, setLabelTextDisplay, updateEditorContent) {
  try {
    console.log('Opening internal SARC file:', selectedPath);
    const content = await invoke('open_internal_file', { path: selectedPath.path });
    setStatusText(content.status_text);
    console.log(content.file_label);
    if (content.tab === 'YAML') {
      setActiveTab(content.tab);
      updateEditorContent(content.text);
      setLabelTextDisplay(prevState => ({ ...prevState, yaml: content.file_label}));
    } else if (content.tab === 'ERROR') {
      console.log("Error opening file, no tab set");
    } else {
      setStatusText("Error: backend sent invalid tab type");
    }
  }
  catch (error) {
    console.error('Failed to open internal SARC file:', error);
  }


}
export async function fetchAndSetEditorContent(setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent) {
  try {
    const content = await invoke('open_file_struct');
    setStatusText(content.status_text);
    if (content.tab === 'SARC') {
      setActiveTab(content.tab);
      setLabelTextDisplay(prevState => ({ ...prevState, sarc: content.file_label}));
      setpaths(content.sarc_paths);
    } else if (content.tab === 'YAML') {
      setActiveTab(content.tab);
      updateEditorContent(content.text);
      setLabelTextDisplay(prevState => ({ ...prevState, yaml: content.file_label}));
    } else if (content.tab === 'ERROR') {
      console.log("Error opening file, no tab set");
    }
  } catch (error) {
    console.error('Failed to fetch editor content from Rust backend:', error);
  }
}

export async function saveFileClick(setStatusText, activeTab, setpaths, editorRef) {
   try {
    // const editorText = editorRef.current ? editorRef.current.getValue() : "";
    if (!editorRef.current) {
      console.log("Editor reference not found");
      return;
    }
    const editorText =  editorRef.current.getValue();
    const save_data = { tab: activeTab, text: editorText };
    const content = await invoke('save_file_struct', {saveData: save_data});
    if (content === null) {
      console.log("No content returned from save_file_struct");
      return;
    }
    if (content.sarc_paths.paths.length > 0) {
      setpaths(content.sarc_paths);
      console.log(content.sarc_paths.added_paths);
      console.log(content.sarc_paths.modded_paths);
    }
    console.log(content);
    setStatusText(content.status_text);
    if (content.tab === 'SARC') {
      //setActiveTab(content.tab);
      //setLabelTextDisplay(prevState => ({ ...prevState, sarc: content.file_label}));
    } else if (content.tab === 'YAML') {
      //setActiveTab(content.tab);
      //updateEditorContent(content.text);
      //setLabelTextDisplay(prevState => ({ ...prevState, yaml: content.file_label}));
    } else if (content.tab === 'ERROR') {
      console.log("Error opening file, no tab set");
    }
  } catch (error) {
    console.error('Failed save data:', error);
  }
}