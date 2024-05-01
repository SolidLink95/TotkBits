import React, { useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { OpenFileFromPath } from './ButtonClicks';

export const useFileDropHandler = (setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent) => {
  const canProcessEvent = useRef(true);

  useEffect(() => {
    const handleFileDrop = async ({ payload }) => {
      if (canProcessEvent.current && payload.length > 0) {
        canProcessEvent.current = false; // Set the flag to false to block processing
        const file = payload[0];

        try {
          OpenFileFromPath(file, setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);
        } catch (error) {
          console.error('Error processing file:', error);
        }

        // Reset the flag after 0.7 seconds
        setTimeout(() => {
          canProcessEvent.current = true;
        }, 700);
      }
    };

    let unlisten = null;

    const setupListener = async () => {
      try {
        const listener = await listen('tauri://file-drop', handleFileDrop);
        unlisten = listener.unlisten;
      } catch (error) {
        console.error('Failed to set up listener:', error);
      }
    };

    setupListener();

    return () => {
      if (unlisten) {
        unlisten().catch(error => console.error('Error unlistening:', error));
      }
    };
  }, []);
};

