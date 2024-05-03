import { invoke } from '@tauri-apps/api/tauri'; // Import Tauri invoke method
import React, { useState } from 'react';
import {searchTextInSarcClick} from './ButtonClicks';

function SearchTextInSarcPrompt({  setStatusText, setpaths,
    searchInSarcQuery, setSearchInSarcQuery,
    isSearchInSarcOpened, setIsSearchInSarcOpened, }) {

    const handleSearchClick = () => {
        searchTextInSarcClick(searchInSarcQuery, setpaths, setStatusText, setSearchInSarcQuery, setIsSearchInSarcOpened);
    };

    const cancelClick = () => {
        setIsSearchInSarcOpened("");
        setIsSearchInSarcOpened(false);
        setStatusText("Search cancelled");
      };

      const canSubmit = searchInSarcQuery !== "";
    if (!isSearchInSarcOpened) {
        return null;
    }
    const okButtonClass = canSubmit ? "modal-footer-button" : "modal-footer-button-disabled";


    return (
        <div className="modal-overlay">
            <div className="modal-content">
                <button className="close-button" onClick={cancelClick}>X</button>
                <div >Search for text pattern in opened SARC file (NOT case sensitive).</div>
                <div >May take a while for large SARC files.</div>
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
                    <button className={okButtonClass} title="Proceed" disabled={!canSubmit} onClick={handleSearchClick}>Search</button>
                    <button className="modal-footer-button" title="Cancel operation" onClick={cancelClick}>Cancel</button>
                </div>
            </div>
        </div>
    );

}

export { SearchTextInSarcPrompt };