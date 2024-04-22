import { invoke } from '@tauri-apps/api/tauri'; // Import Tauri invoke method

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
      setStatusText("Select some file first!");
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

export async function searchTextInSarcClick(searchInSarcQuery, setpaths, setStatusText, setSearchInSarcQuery, setIsSearchInSarcOpened) {
  try {
    if (searchInSarcQuery === "") {
      setStatusText("Search query is empty!");
      return;
    }
    setStatusText("Searching in SARC file...");
    const content = await invoke('search_in_sarc', { query: searchInSarcQuery });
    if (content === null) {
      setStatusText("No content returned from search_in_sarc");
      setIsSearchInSarcOpened(false);
      setSearchInSarcQuery("");
      return;
    }
    if (content.sarc_paths.paths.length === 0) {
      setStatusText(`No matches found: ${searchInSarcQuery}`);
      setSearchInSarcQuery("");
    } else {
      setStatusText(content.status_text);
      setpaths(content.sarc_paths);
    }
    setIsSearchInSarcOpened(false);
  } catch (error) {
    console.error("Error invoking 'add_click':", error);
  }
}
export async function clearSearchInSarcClick(setpaths, setStatusText, setSearchInSarcQuery) {
  try {
    const content = await invoke('clear_search_in_sarc');
    if (content !== null) {
      setStatusText(content.status_text);
    }
    setSearchInSarcQuery("");
    setpaths(content.sarc_paths);
  }
  catch (error) {
    console.error('Failed to clear search in sarc file:', error);
  }
}


export async function editInternalSarcFile(fullPath, setStatusText, setActiveTab, setLabelTextDisplay, updateEditorContent) {
  try {
    if (fullPath === null || fullPath === undefined || fullPath === "") {
      setStatusText("Select some file first!");
      return;
    }
    console.log('Opening internal SARC file:', fullPath);
    setStatusText("Opening...");
    const content = await invoke('edit_internal_file', { path: fullPath });
    if (content === null) {
      setStatusText("Ready");
      // setStatusText("No content returned! Is any SARC file opened?");
      return;
    }
    //  setStatusText(content.status_text);
    console.log(content.file_label);
    if (content.tab === 'YAML') {
      setActiveTab(content.tab);
      updateEditorContent(content.text, content.lang);
      setLabelTextDisplay(prevState => ({ ...prevState, yaml: content.file_label }));
      setStatusText(`Opened file: ${fullPath}`);
    } else if (content.tab === 'ERROR') {
      console.log("Error opening file, no tab set");
      setStatusText("Unsupported file type");
    } else {
      setStatusText("Error: backend sent invalid tab type");
    }
  }
  catch (error) {
    console.error('Failed to open internal SARC file:', error);
    setStatusText('Failed to open internal SARC file:', error);
  }


}
export async function OpenFileFromPath(argv1, setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent) {
  try {
    const content = await invoke('open_file_from_path', { path: argv1 });
    if (content === null) {
      console.log("No content returned from process_argv");
      return;
    }
    setStatusText(content.status_text);
    if (content.tab === 'SARC') {
      setActiveTab(content.tab);
      setLabelTextDisplay(prevState => ({ ...prevState, sarc: content.file_label }));
      setpaths(content.sarc_paths);
    } else if (content.tab === 'YAML') {
      setActiveTab(content.tab);
      updateEditorContent(content.text, content.lang);
      setLabelTextDisplay(prevState => ({ ...prevState, yaml: content.file_label }));
    } else if (content.tab === 'RSTB') {
      setActiveTab(content.tab);
      setLabelTextDisplay(prevState => ({ ...prevState, rstb: content.file_label }));

    } else if (content.tab === 'ERROR') {
      console.log("Error opening file, no tab set");
    }

  } catch (error) {
    console.error('Failed to process argv[1]:', error);
  }
}
export async function fetchAndSetEditorContent(setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent) {
  try {
    setStatusText("Opening file...");
    const content = await invoke('open_file_struct');
    setStatusText(content.status_text);
    if (content.tab === 'SARC') {
      setActiveTab(content.tab);
      setLabelTextDisplay(prevState => ({ ...prevState, sarc: content.file_label }));
      setpaths(content.sarc_paths);
      updateEditorContent("", content.lang);
    } else if (content.tab === 'YAML') {
      setActiveTab(content.tab);
      updateEditorContent(content.text, content.lang);
      console.log(content.lang);
      setLabelTextDisplay(prevState => ({ ...prevState, yaml: content.file_label }));
    } else if (content.tab === 'RSTB') {
      setActiveTab(content.tab);
      setLabelTextDisplay(prevState => ({ ...prevState, rstb: content.file_label }));

    } else if (content.tab === 'ERROR') {
      console.log("Error opening file, no tab set");
      setStatusText("Error opening file");
    }
  } catch (error) {
    console.error('Failed to fetch editor content from Rust backend:', error);
    
    setStatusText("");
  }
}

export async function closeAllFilesClick(setStatusText, setpaths, updateEditorContent, setLabelTextDisplay) {
  try {
    const content = await invoke('close_all_opened_files');
    if (content === null) {
      console.log("No content returned from close_all_files");
      return;
    }
    setStatusText(content.status_text);
    setpaths(content.sarc_paths);
    updateEditorContent(content.text, content.lang);
    setLabelTextDisplay({ sarc: '', yaml: '', rstb: '' });
  } catch (error) {
    console.error('Failed to close all files:', error);
  }

}

export async function editConfigFileClick(setStatusText) {
  try {
    const content = await invoke('edit_config');
    setStatusText("Config file opened. Restart program for changes to take effect.")
  } catch (error) {
    console.error('Failed to edit_config: ', error);
  }

}

export async function restartApp(setStatusText) {
  try {
    const content = await invoke('restart_app');
    setStatusText("App restart aborted.")
  } catch (error) {
    console.error('Failed to edit_config: ', error);
  }

}

export async function removeInternalFileClick(internalPath, setStatusText, setpaths) {
  try {
    const content = await invoke('remove_internal_sarc_file', { internalPath: internalPath });
    if (content === null) {
      console.log("No content returned from remove_internal_sarc_file");
      return;
    }
    setStatusText(content.status_text);
    if (content.sarc_paths.paths.length > 0) {
      setpaths(content.sarc_paths);
    }

  } catch (error) {
    console.error("Error invoking 'remove_internal_file':", error);
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

export async function addInternalFileToDir(internalPath, setStatusText, setpaths) {
  try {
    const path = await invoke("open_file_dialog");
    if (path === "" || path === null || path === undefined) {
      return;
    }
    const content = await invoke('add_to_dir_click', { internalPath: internalPath, path: path });
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
    setStatusText("Saving...");
    if (!editorRef.current) {
      console.log("Editor reference not found");
      setStatusText("Editor reference not found");
      return;
    }
    const editorText = editorRef.current.getValue();
    const save_data = { tab: activeTab, text: editorText };
    // console.log("About to save");
    // console.log(save_data);
    const content = await invoke('save_file_struct', { saveData: save_data });
    // console.log("received content from save_file_struct:");
    // console.log(content);
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
    setStatusText(`Failed to save data`);
  }
}

export async function saveAsFileClick(setStatusText, activeTab, setpaths, editorRef) {
  try {
    // const editorText = editorRef.current ? editorRef.current.getValue() : "";
    if (!editorRef.current) {
      console.log("Editor reference not found");
      return;
    }
    const editorText = editorRef.current.getValue();
    const save_data = { tab: activeTab, text: editorText };
    console.log(save_data);
    const content = await invoke('save_as_click', { saveData: save_data });
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