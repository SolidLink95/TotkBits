import { invoke } from '@tauri-apps/api/tauri'; // Import Tauri invoke method
import React, { useState } from 'react';

function AddOrRenameFilePrompt({ isOpen, onClose, setStatusText, setpaths,
  isAddPrompt, selectedPath, renamePromptMessage }) {
  const name = selectedPath.path.split("/").pop();
  const [internal_path, setInternalSarcPath] = useState('');
  const [path, setFilePath] = useState('');
  // const [isAddPrompt, setIsAddPrompt] = useState(true);

  const cancelClick = () => {
    setInternalSarcPath("");
    setFilePath("");
    onClose();
  };

  const handleRenameOkClick = async (internalPath) => {
    try {
      const content = await invoke('rename_internal_sarc_file', { internalPath: selectedPath.path, newInternalPath: internalPath });
      if (content === null) {
        console.log("No content returned from rename_internal_sarc_file");
        cancelClick();
        return;
      }
      setStatusText(content.status_text);
      if (content.sarc_paths.paths.length > 0) {
        setpaths(content.sarc_paths);
      }
      setInternalSarcPath("");
      setFilePath("");
      onClose();
    } catch (error) {
      console.error("Error invoking 'rename_click':", error);
    }

  }
  const handleAddOkClick = async (internalPath, path) => {
    try {
      console.log("internal_path", internalPath);
      console.log("path", path);
      const content = await invoke('add_click', { internalPath: internalPath, path: path, overwrite: false });
      if (content === null) {
        console.log("No content returned from add_click");
        cancelClick();
        return;
      }
      setStatusText(content.status_text);
      if (content.sarc_paths.paths.length > 0) {
        setpaths(content.sarc_paths);
      }
      setInternalSarcPath("");
      setFilePath("");
      onClose();
    } catch (error) {
      console.error("Error invoking 'add_click':", error);
    }
  };

  const handleSelectFileClick = async () => {
    const content = await invoke("open_file_dialog");
    if (content !== null) {
      console.log(content);
      setFilePath(content);
    }
  };

  if (!isOpen || isAddPrompt === null) return null;
  // console.log("isAddPrompt", isAddPrompt);
  const canSubmit = (isAddPrompt && path && internal_path.length > 0 && internal_path.replace("//", "/").split("/").pop().includes(".") &&
    path.length > 0) || (!isAddPrompt && internal_path.length > 0);
  const okButtonClass = canSubmit ? "modal-footer-button" : "modal-footer-button-disabled";

  if (isAddPrompt) {
    return (
      <div className="modal-overlay">
        <div className="modal-content">
          <button className="close-button" onClick={onClose}>X</button>
          <div>Select internal sarc path and file to be added.</div>
          <div className="modal-header">Internal sarc path must contain "." in base name</div>
          <div className="modal-row">
            <input
              type="text"
              placeholder='Full path inside sarc'
              className="modal-input"
              value={internal_path}
              onChange={(e) => setInternalSarcPath(e.target.value)}
            />
          </div>
          <div className="modal-row">
            <input
              type="text"
              placeholder='Path to file'
              className="modal-input"
              style={{ marginRight: '5px' }}
              value={path}
              onChange={(e) => setFilePath(e.target.value)}
            />
            <button className="button" onClick={handleSelectFileClick}>Select file</button>
          </div>
          <div className="modal-footer">
            <button className={okButtonClass} title="Proceed" disabled={!canSubmit} onClick={() => handleAddOkClick(internal_path, path)}>Ok</button>
            <button className="modal-footer-button" title="Cancel operation" onClick={cancelClick}>Cancel</button>
          </div>
        </div>
      </div>
    );
  } else {
    return (
      <div className="modal-overlay">
        <div className="modal-content">
          <button className="close-button" onClick={onClose}>X</button>
          <div >{renamePromptMessage.message}</div>
          <div className="modal-header">{renamePromptMessage.path}</div>
          <div className="modal-row">
            <input
              type="text"
              placeholder='New name'
              className="modal-input"
              value={internal_path}
              onChange={(e) => setInternalSarcPath(e.target.value)}
            />
          </div>
          <div className="modal-footer">
            <button className={okButtonClass} title="Proceed" disabled={!canSubmit} onClick={() => handleRenameOkClick(internal_path)}>Ok</button>
            <button className="modal-footer-button" title="Cancel operation" onClick={cancelClick}>Cancel</button>
          </div>
        </div>
      </div>
    );
  }
}

export default AddOrRenameFilePrompt;
