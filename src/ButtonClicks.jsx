import { getDocumentsSnapshot, invoke } from './DocumentState';
import { flushSync } from 'react-dom';
import * as monaco from 'monaco-editor';
import { addRflMii, replaceRflMii } from './Mii';


export async function addEmptyByml(fullPath,setStatusText, setpaths) {
  try {
    const content = await invoke('add_empty_byml_file', { path: fullPath });
    console.log(content);
    if (content !== null && content.status_text !== undefined) {
      setStatusText(content.status_text);
      setpaths(content.sarc_paths);
    }
  } catch (error) {
    console.error('Failed to add empty byml: ', error);
  }
}


export const useExitApp = async () => {
  console.log('Exiting the app');
  try {
    invoke('exit_app');
  } catch (error) {
    console.error(error);
  }
};

export async function checkIfUpdateNeeded(setUpdateState) {
  try {

    const content = await invoke('check_if_update_needed');
    if (content === null || content === undefined) {
      console.log("No content returned from check_if_update_needed");
      setUpdateState({wasChecked: true, isUpdateNeeded: false, latestVersion: ''});
      return;
    }
    const latestVersion = content;
    const isUpdateNeeded = latestVersion !== null && latestVersion !== undefined && latestVersion !== '' && latestVersion !== '0.0.0' ? true : false;
    
    // const result = content === null ? false : content === "yes" ? true : false;
    setUpdateState({wasChecked: true, isUpdateNeeded: isUpdateNeeded, latestVersion: latestVersion})
    // console.log('Update needed: ', content);
  }
  catch (error) {
    console.error('Failed to check update:', error);
  }
}
export async function extractFileClick(selectedPath, setStatusText) {
  try {
    const path = selectedPath.path ?? '';
    if (path === "") {
      setStatusText("Select some file first!");
      return;
    }

    const content = await invoke('extract_internal_file', { internalPath: path });
    if (content !== null) {
      setStatusText(content.status_text);
    }
  }
  catch (error) {
    console.error('Failed to extract file:', error);
  }
}

export async function extractFolderClick(sourcePath, setStatusText) {
  try {
    // if (sourcePath === "") {
    //   setStatusText("Select some folder first!");
    //   return;
    // }
    console.log('Extracting folder:', sourcePath);
    const content = await invoke('extract_folder_from_opened_sarc', { sourceFolder: sourcePath });
    if (content !== null) {
      setStatusText(content.status_text);
    }
  }
  catch (error) {
    console.error('Failed to extract folder:', error);
  }
}

export async function extractRootFolderClick(setStatusText) {
  return extractFolderClick("", setStatusText);
}

const persistArchiveSearch = (paths, query, documentSnapshots) => {
  const { activeDocumentId } = getDocumentsSnapshot();
  const snapshot = documentSnapshots?.current?.get(activeDocumentId);
  if (snapshot) {
    documentSnapshots.current.set(activeDocumentId, {
      ...snapshot,
      paths,
      searchInSarcQuery: query,
    });
  }
};

export async function searchTextInSarcClick(searchInSarcQuery, setpaths, setStatusText, setSearchInSarcQuery, setIsSearchInSarcOpened, documentSnapshots) {
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
      persistArchiveSearch(content.sarc_paths, searchInSarcQuery, documentSnapshots);
    }
    setIsSearchInSarcOpened(false);
  } catch (error) {
    console.error("Error invoking 'add_click':", error);
  }
}
export async function clearSearchInSarcClick(setpaths, setStatusText, setSearchInSarcQuery, documentSnapshots) {
  try {
    const content = await invoke('clear_search_in_sarc');
    if (content !== null) {
      setStatusText(content.status_text);
    }
    setSearchInSarcQuery("");
    setpaths(content.sarc_paths);
    persistArchiveSearch(content.sarc_paths, '', documentSnapshots);
  }
  catch (error) {
    console.error('Failed to clear search in sarc file:', error);
  }
}


export async function editInternalSarcFile(fullPath, setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent) {
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
    console.log('content.file_label', content.file_label);
    if (content.tab === 'SARC') {
      setLabelTextDisplay(prevState => ({ ...prevState, sarc: content.file_label.replace(/\/\//g, '/') }));
      setpaths(content.sarc_paths);
      setStatusText(content.status_text);
      setActiveTab(content.tab);
    } else if (content.tab === 'YAML') {
      setLabelTextDisplay(prevState => ({ ...prevState, yaml: content.file_label.replace(/\/\//g, '/') }));
      updateEditorContent(content.text, content.lang);
      setStatusText(`Opened file: ${fullPath}`);
      setActiveTab(content.tab);
    } else if (content.tab === '3D') {
      setStatusText(content.status_text || `Opened file: ${fullPath}`);
      setActiveTab(content.tab);
    } else if (content.tab === 'IMAGE') {
      setStatusText(content.status_text || `Opened file: ${fullPath}`);
      setActiveTab(content.tab);
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

export async function openBphclLeaf(fullPath, parentDocumentId, setStatusText, setActiveTab, setLabelTextDisplay, updateEditorContent, setReadOnly) {
  try {
    const content = await invoke('open_bphcl_leaf', { path: fullPath, parentDocumentId });
    if (!content) {
      setStatusText(`Unable to preview BPHCL node: ${fullPath}`);
      return;
    }
    setLabelTextDisplay(prev => ({ ...prev, yaml: content.file_label }));
    updateEditorContent(content.text, content.lang || 'yaml');
    setReadOnly(content.read_only ?? true);
    setActiveTab('YAML');
    setStatusText(content.status_text);
  } catch (error) {
    setStatusText(`Unable to preview BPHCL node: ${String(error)}`);
  }
}

export async function removeBphclNodeClick(fullPath, setStatusText, setpaths) {
  try {
    const content = await invoke('remove_bphcl_node', { path: fullPath });
    setpaths(content.sarcPaths);
    setStatusText(content.statusText);
    return true;
  } catch (error) {
    setStatusText(`ERROR: ${String(error)}`);
    return false;
  }
}

export async function expandNestedSarc(outerPath, setStatusText, setpaths, setPathsFilters) {
  const content = await invoke('expand_nested_sarc', { outerPath });
  if (!content) return false;
  setStatusText(content.status_text);
  if (content.tab !== 'ERROR') {
    setPathsFilters?.({ showAll: true, showAdded: false, showModded: false });
    setpaths(content.sarc_paths);
    return true;
  }
  return false;
}

export async function editNestedSarcFile(outerPath, innerPath, setStatusText, setActiveTab, setLabelTextDisplay, updateEditorContent) {
  const content = await invoke('edit_nested_sarc_file', { outerPath, innerPath });
  if (!content) return;
  setStatusText(content.status_text);
  if (content.tab === 'YAML') {
    updateEditorContent(content.text, content.lang);
    setLabelTextDisplay((previous) => ({ ...previous, yaml: content.file_label }));
    setActiveTab('YAML');
  } else if (content.tab === 'SARC') {
    setActiveTab('SARC');
  } else if (content.tab === '3D' || content.tab === 'IMAGE') {
    setActiveTab(content.tab);
  }
}

export async function extractNestedSarcFile(outerPath, innerPath, setStatusText) {
  const content = await invoke('extract_nested_sarc_file', { outerPath, innerPath });
  if (content) setStatusText(content.status_text);
}
export async function mutateNestedArchive(chain, path, action, setStatusText, setpaths, options = {}) {
  const content = await invoke('mutate_nested_archive', { chain, path, action, newPath: options.newPath ?? null, sourcePath: options.sourcePath ?? null });
  if (content) { setStatusText(content.status_text); if (content.tab !== 'ERROR') setpaths(content.sarc_paths); }
  return content;
}
export async function OpenFileFromPath(argv1, setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent, silent = false) {
  try {
    setStatusText("Opening file...");
    const content = await invoke('open_file_from_path', { path: argv1, suppressErrorDialog: silent });
    if (content === null) {
      console.log("No content returned from process_argv");
      if (!silent) setStatusText("Error: unable to open file: " + argv1);
      return false;
    }
    flushSync(() => {
      setStatusText(content.status_text);
      if (content.tab === 'SARC') {
        setActiveTab(content.tab);
        setLabelTextDisplay(prevState => ({ ...prevState, sarc: content.file_label.replace(/\/\//g, '/') }));
        setpaths(content.sarc_paths);
      } else if (content.tab === 'YAML') {
        setActiveTab(content.tab);
        updateEditorContent(content.text, content.lang, content.read_only ?? false);
        setLabelTextDisplay(prevState => ({ ...prevState, yaml: content.file_label.replace(/\/\//g, '/') }));
      } else if (content.tab === 'RSTB') {
        setActiveTab(content.tab);
        setLabelTextDisplay(prevState => ({ ...prevState, rstb: content.file_label.replace(/\/\//g, '/') }));
      } else if (content.tab === '3D') {
        setActiveTab(content.tab);
      } else if (content.tab === 'IMAGE') {
        setActiveTab(content.tab);
      } else if (content.tab === 'ERROR') {
        console.log("Error opening file, no tab set");
      }
    });

  } catch (error) {
    console.error('Failed to process argv[1]:', error);
      if (!silent) setStatusText("Error: failed to open file: " + argv1);
      return false;
  }
  return true;
}
export async function fetchAndSetEditorContent(setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent) {
  try {
    // setActiveTab("LOADING");
    setStatusText("Opening file...");
    const content = await invoke('open_file_struct');
    if (!content) {
      setStatusText("Ready");
      return;
    }
    setStatusText(content.status_text);
    if (content.tab === 'SARC') {
      setActiveTab(content.tab);
      setLabelTextDisplay(prevState => ({ ...prevState, sarc: content.file_label.replace(/\/\//g, '/') }));
      setpaths(content.sarc_paths);
      updateEditorContent("", content.lang);
    } else if (content.tab === 'YAML') {
      setActiveTab(content.tab);
      updateEditorContent(content.text, content.lang, content.read_only ?? false);
      console.log(content.lang);
      setLabelTextDisplay(prevState => ({ ...prevState, yaml: content.file_label.replace(/\/\//g, '/') }));
    } else if (content.tab === 'RSTB') {
      setActiveTab(content.tab);
      setLabelTextDisplay(prevState => ({ ...prevState, rstb: content.file_label.replace(/\/\//g, '/') }));
    } else if (content.tab === '3D') {
      setActiveTab(content.tab);
    } else if (content.tab === 'IMAGE') {
      setActiveTab(content.tab);
    } else if (content.tab === 'ERROR') {
      // setActiveTab(activeTabBak);
      console.log("Error opening file, no tab set");
      setStatusText("Error opening file");
    }
  } catch (error) {
    console.error('Failed to fetch editor content from Rust backend:', error);

    setStatusText("");
  }
  // setActiveTab(activeTabBak);
}

export async function openFolderContent(setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent) {
  try {
    setStatusText('Opening folder...');
    const content = await invoke('open_folder_struct');
    if (!content) { setStatusText('Ready'); return; }
    setStatusText(content.status_text);
    setActiveTab('SARC');
    setLabelTextDisplay((previous) => ({ ...previous, sarc: content.file_label.replace(/\/\//g, '/') }));
    setpaths(content.sarc_paths);
    updateEditorContent('', content.lang);
  } catch (error) {
    console.error('Failed to open folder:', error);
    setStatusText(`Error: failed to open folder: ${error}`);
  }
}

export async function closeAllFilesClick(setCompareData, setStatusText, setpaths, updateEditorContent, setLabelTextDisplay) {
  try {
    const content = await invoke('close_all_opened_files');
    if (!content) {
      console.log("No content returned from close_all_files");
      return;
    }
    setStatusText(content.status_text);
    setpaths(content.sarc_paths);
    updateEditorContent(content.text, content.lang);
    setLabelTextDisplay({ sarc: '', yaml: '', rstb: '', comparer: '' });
    setCompareData({ decision: 'FilesFromDisk', content1: '', content2: '', filepath1: '', filepath2: '', isSmall: true, isFromDisk: false, isInternal: false, label1: '', label2: '' });
    
    // setCompareData({ decision: 'FilesFromDisk', content1: '', content2: '', filepath1: '', filepath2: '', isSmall: true, isFromDisk: false, isInternal: false, label1: '', label2: '' });
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
    setpaths(content.sarc_paths);

  } catch (error) {
    console.error("Error invoking 'remove_internal_file':", error);
  }
}

export async function replaceInternalFileClick(internalPath, setStatusText, setpaths, archiveType = '') {
  try {
    const path = await invoke("open_file_dialog");
    if (path === null || path === undefined || path === "") {
      return;
    }
    let content;
    if (archiveType === 'RFL_DB') {
      content = await replaceRflMii(internalPath, path, setStatusText);
    } else {
      content = await invoke('add_click', { internalPath: internalPath, path: path, overwrite: true });
    }
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
    setStatusText(`Error replacing archive file: ${String(error)}`);
  }

}

export async function addInternalFileToDir(internalPath, setStatusText, setpaths, archiveType = '') {
  try {
    const path = await invoke("open_file_dialog");
    if (path === "" || path === null || path === undefined) {
      return;
    }
    let content;
    if (archiveType === 'RFL_DB' && /\.(?:charinfo|ltd)$/i.test(path)) {
      content = await addRflMii(internalPath, path, setStatusText);
    } else {
      content = await invoke('add_to_dir_click', { internalPath: internalPath, path: path });
    }
    if (content === null) {
      console.log("No content returned from add_click");
      return;
    }
    setStatusText(content.status_text);
    if (content.sarc_paths.paths.length > 0) {
      setpaths(content.sarc_paths);
    }
  } catch (error) {
    console.error("Error invoking 'addInternalFileToDir':", error);
    setStatusText(`Error adding archive file: ${String(error)}`);
  }

}

export async function addFilesFromDirRecursivelyToRoot(setStatusText, setpaths) {
  return addFilesFromDirRecursively("", setStatusText, setpaths);
}

export async function addFilesFromDirRecursively(internalPath, setStatusText, setpaths) {
  try {
    const path = await invoke("open_dir_dialog");
    if (path === "" || path === null || path === undefined) {
      return;
    }
    const content = await invoke('add_files_from_dir_recursively', { internalPath: internalPath, path: path });
    if (content === null) {
      console.log("No content returned from add_click");
      return;
    }
    setStatusText(content.status_text);
    if (content.sarc_paths.paths.length > 0) {
      setpaths(content.sarc_paths);
    }
  } catch (error) {
    console.error("Error invoking 'addFilesFromDirRecursively':", error);
  }

}

const refreshSavedArchivePaths = (sarcPaths, setpaths, documentSnapshots) => {
  if (!sarcPaths || sarcPaths.paths.length === 0) return;
  setpaths(sarcPaths);
  const { documents, activeDocumentId } = getDocumentsSnapshot();
  if (!activeDocumentId || !documentSnapshots?.current) return;
  const activeSnapshot = documentSnapshots.current.get(activeDocumentId);
  if (activeSnapshot) {
    documentSnapshots.current.set(activeDocumentId, { ...activeSnapshot, paths: sarcPaths });
  }
  const activeDocument = documents.find((document) => document.id === activeDocumentId);
  const parentId = activeDocument?.parentDocumentId;
  if (parentId && activeDocument?.fileMetadata?.includes('[BPHCL] [AAMP]')) {
    const parentSnapshot = documentSnapshots.current.get(parentId);
    if (parentSnapshot) {
      documentSnapshots.current.set(parentId, { ...parentSnapshot, paths: sarcPaths });
    }
  } else if (parentId && sarcPaths) {
    const parentSnapshot = documentSnapshots.current.get(parentId);
    if (parentSnapshot) {
      documentSnapshots.current.set(parentId, {
        ...parentSnapshot,
        paths: sarcPaths,
      });
    }
  }
};

const activeFileName = () => {
  const { documents, activeDocumentId } = getDocumentsSnapshot();
  return documents.find((document) => document.id === activeDocumentId)?.title || 'file';
};

export async function saveFileClick(setStatusText, activeTab, setpaths, editorRef, setSavingFile, documentSnapshots) {
  setSavingFile?.(activeFileName());
  try {
    await new Promise((resolve) => requestAnimationFrame(resolve));
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
      setStatusText("Ready");
      return;
    }
    if (content.sarc_paths.paths.length > 0) {
      refreshSavedArchivePaths(content.sarc_paths, setpaths, documentSnapshots);
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
  } finally {
    setSavingFile?.('');
  }
}

export async function saveAsFileClick(setStatusText, activeTab, setpaths, editorRef, setSavingFile, documentSnapshots) {
  setSavingFile?.(activeFileName());
  try {
    await new Promise((resolve) => requestAnimationFrame(resolve));
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
      setStatusText("Ready");
      return;
    }
    if (content.sarc_paths.paths.length > 0) {
      refreshSavedArchivePaths(content.sarc_paths, setpaths, documentSnapshots);
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
  } finally {
    setSavingFile?.('');
  }
}


