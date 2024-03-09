import React from 'react';
import { app } from '@tauri-apps/api';
import { invoke } from '@tauri-apps/api/tauri'; // Import Tauri invoke method

export const useExitApp = async () => {
  console.log('Exiting the app');
  try {
    invoke('exit_app');
  } catch (error) {
    console.error(error);
  }
};

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