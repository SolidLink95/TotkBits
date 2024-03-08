import React from 'react';
import { app } from '@tauri-apps/api';

export const useExitApp = async () => {
    console.log('Exiting the app');
    await app.exit();
  };