import { invoke } from '@tauri-apps/api/tauri'; // Import Tauri invoke method
import React, { useState, useEffect } from 'react';

function AddOrRenameFilePrompt({
  isOpen,
  onClose,
  setStatusText,
  setpaths,
  isAddPrompt,
  selectedPath,
  renamePromptMessage,
}) {
  const [internalPath, setInternalPath] = useState('');
  const [filePath, setFilePath] = useState('');

  // Update internalPath when the dialog opens or selectedPath changes
  useEffect(() => {
    if (isOpen && selectedPath?.path) {
      setInternalPath(selectedPath.path);
    }
  }, [isOpen, selectedPath]);

  const cancelClick = () => {
    setInternalPath('');
    setFilePath('');
    onClose();
  };

  const handleRenameOkClick = async (internalPath) => {
    try {
      const content = await invoke('rename_internal_sarc_file', {
        internalPath: selectedPath.path,
        newInternalPath: internalPath,
      });
      if (!content) {
        console.log("No content returned from rename_internal_sarc_file");
        cancelClick();
        return;
      }
      setStatusText(content.status_text);
      if (content.sarc_paths.paths.length > 0) {
        setpaths(content.sarc_paths);
      }
      cancelClick();
    } catch (error) {
      console.error("Error invoking 'rename_click':", error);
    }
  };

  const handleAddOkClick = async (internalPath, filePath) => {
    try {
      const content = await invoke('add_click', {
        internalPath,
        path: filePath,
        overwrite: false,
      });
      if (!content) {
        console.log("No content returned from add_click");
        cancelClick();
        return;
      }
      setStatusText(content.status_text);
      if (content.sarc_paths.paths.length > 0) {
        setpaths(content.sarc_paths);
      }
      cancelClick();
    } catch (error) {
      console.error("Error invoking 'add_click':", error);
    }
  };

  const handleSelectFileClick = async () => {
    try {
      const content = await invoke('open_file_dialog');
      if (content) {
        console.log(content);
        setFilePath(content);
      }
    } catch (error) {
      console.error('Error opening file dialog:', error);
    }
  };

  if (!isOpen || isAddPrompt === null) return null;

  const canSubmit =
    (isAddPrompt && internalPath.includes('.') && filePath) ||
    (!isAddPrompt && internalPath);

  const okButtonClass = canSubmit
    ? 'modal-footer-button'
    : 'modal-footer-button-disabled';

  if (isAddPrompt) {
    return (
      <div className="modal-overlay">
        <div className="modal-content">
          <button className="close-button" onClick={onClose}>
            X
          </button>
          <div>Select internal sarc path and file to be added.</div>
          <div className="modal-header">
            Internal sarc path must contain "." in base name
          </div>
          <div className="modal-row">
            <input
              type="text"
              placeholder="Full path inside sarc"
              className="modal-input"
              value={internalPath}
              onChange={(e) => setInternalPath(e.target.value)}
            />
          </div>
          <div className="modal-row">
            <input
              type="text"
              placeholder="Path to file"
              className="modal-input"
              style={{ marginRight: '5px' }}
              value={filePath}
              onChange={(e) => setFilePath(e.target.value)}
            />
            <button className="generic_button" onClick={handleSelectFileClick}>
              Select file
            </button>
          </div>
          <div className="modal-footer">
            <button
              className={okButtonClass}
              title="Proceed"
              disabled={!canSubmit}
              onClick={() => handleAddOkClick(internalPath, filePath)}
            >
              Ok
            </button>
            <button
              className="modal-footer-button"
              title="Cancel operation"
              onClick={cancelClick}
            >
              Cancel
            </button>
          </div>
        </div>
      </div>
    );
  } else if (renamePromptMessage.path.length > 0) {
    return (
      <div className="modal-overlay">
        <div className="modal-content">
          <button className="close-button" onClick={onClose}>
            X
          </button>
          <div>{renamePromptMessage.message}</div>
          <div className="modal-header">{renamePromptMessage.path}</div>
          <div className="modal-row">
            <input
              type="text"
              placeholder="Full path inside sarc"
              className="modal-input"
              value={internalPath}
              onChange={(e) => setInternalPath(e.target.value)}
            />
          </div>
          <div className="modal-footer">
            <button
              className={okButtonClass}
              title="Proceed"
              disabled={!canSubmit}
              onClick={() => handleRenameOkClick(internalPath)}
            >
              Ok
            </button>
            <button
              className="modal-footer-button"
              title="Cancel operation"
              onClick={cancelClick}
            >
              Cancel
            </button>
          </div>
        </div>
      </div>
    );
  }
}

export default AddOrRenameFilePrompt;
