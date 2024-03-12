import { invoke } from '@tauri-apps/api/tauri'; // Import Tauri invoke method
import { useEditorContext } from './StateManager';
import { path } from '@tauri-apps/api';

export const useExitApp = async () => {
  console.log('Exiting the app');
  try {
    invoke('exit_app');
  } catch (error) {
    console.error(error);
  }
};

export async function extractFileClick(selectedPath, setStatusText) {
  try {
    if (selectedPath.path === null || selectedPath.path === undefined || selectedPath.path === "") { 
      return;
    }

    const content = await invoke('extract_internal_file', { internalPath: selectedPath.path });
    if (content !== null) {
      setStatusText(content.status_text);
    }
  }
  catch (error) {
    console.error('Failed to extract file:', error);
  }
}


export async function editInternalSarcFile(fullPath, setStatusText, setActiveTab, setLabelTextDisplay, updateEditorContent) {
  try {
    console.log('Opening internal SARC file:', fullPath);
    const content = await invoke('edit_internal_file', { path: fullPath });
  //  setStatusText(content.status_text);
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



export async function replaceInternalFileClick(internalPath, setStatusText, setpaths) {
  try {
    const path = await invoke("open_file_dialog");
    if (path === null || path === undefined || path === "") {
      return;
    }
    const content = await invoke('add_click', { internalPath: internalPath, path: path, overwrite: true });
    if (content === null) {
      console.log("No content returned from add_click");
      return;
    }
    setStatusText(content.status_text);
    if (content.sarc_paths.paths.length > 0) {
      setpaths(content.sarc_paths);
    }
  } catch (error) {
    console.error("Error invoking 'add_click':", error);
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
    if (content.tab === 'ERROR') {
      console.log("Error opening file, no tab set");
    }
  } catch (error) {
    console.error('Failed save data:', error);
  }
}

export async function saveAsFileClick(setStatusText, activeTab, setpaths, editorRef) {
  try {
   // const editorText = editorRef.current ? editorRef.current.getValue() : "";
   if (!editorRef.current) {
     console.log("Editor reference not found");
     return;
   }
   const editorText =  editorRef.current.getValue();
   const save_data = { tab: activeTab, text: editorText };
   console.log(save_data );
   const content = await invoke('save_as_click', {saveData: save_data});
   if (content === null) {
     console.log("No content returned from save_as_click");
     return;
   }
   if (content.sarc_paths.paths.length > 0) {
     setpaths(content.sarc_paths);
     console.log(content.sarc_paths.added_paths);
     console.log(content.sarc_paths.modded_paths);
   }
   console.log(content);
   setStatusText(content.status_text);
   if (content.tab === 'ERROR') {
     console.log("Error opening file, no tab set");
   }
 } catch (error) {
   console.error('Failed save as data: ', error);
 }
}


export const simulateEscapeKeyPress = () => {
  // Create a new event
  const event = new KeyboardEvent('keydown', {
    key: 'Escape',
    code: 'Escape',
    keyCode: 27, // Deprecated, but included for compatibility with older browsers
    which: 27, // Deprecated, but included for compatibility with older browsers
    bubbles: true, // Event bubbles up through the DOM
    cancelable: true, // Event can be canceled
  });

  // Dispatch the event on the document or a specific element
  document.dispatchEvent(event);
};