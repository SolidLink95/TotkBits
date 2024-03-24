import { invoke } from '@tauri-apps/api/tauri'; // Import Tauri invoke method
import React, { useState } from 'react';

function AddOrRenameFilePrompt({  setStatusText, setpaths,
    searchInSarcQuery, setSearchInSarcQuery,
    isSearchInSarcOpened, setIsSearchInSarcOpened, }) {

    const handleSearchClick = async () => {
        try {
            if (searchInSarcQuery === "") {
                setStatusText("Search query is empty!");
                return;
            }
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

    const cancelClick = () => {
        setIsSearchInSarcOpened("");
        setIsSearchInSarcOpened(false);
      };

      const canSubmit = searchInSarcQuery !== "";
    if (!isSearchInSarcOpened) {
        return null;
    }


    return (
        <div className="modal-overlay">
            <div className="modal-content">
                <button className="close-button" onClick={onClose}>X</button>
                <div >Search for text pattern in opened SARC file. Search is NOT case sensitive</div>
                {/* <div className="modal-header">{renamePromptMessage.path}</div> */}
                <div className="modal-row">
                    <input
                        type="text"
                        placeholder='Type here and press search button'
                        className="modal-input"
                        value={searchInSarcQuery}
                        onChange={(e) => setSearchInSarcQuery(e.target.value)}
                    />
                </div>
                <div className="modal-footer">
                    <button className={okButtonClass} title="Proceed" disabled={!canSubmit} onClick={() => handleRenameOkClick(internal_path)}>Search</button>
                    <button className="modal-footer-button" title="Cancel operation" onClick={cancelClick}>Cancel</button>
                </div>
            </div>
        </div>
    );

}

export default AddOrRenameFilePrompt;
