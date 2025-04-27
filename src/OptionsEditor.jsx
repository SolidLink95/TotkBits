import { invoke } from "@tauri-apps/api/tauri";
import React, { useState, useEffect } from "react";
import { useEditorContext } from "./StateManager";

function OptionsEditor() {
    const {
        settings,
        setSettings,
        statusText,
        setStatusText,
        setIsOptionsOpen,
        setConfig,
        setConfigLoading,
        isOptionsOpen,
        config,
        configLoading,
        setIsModalOpen,
        isModalOpen,
    } = useEditorContext();

    const updateRomfsPath = async () => {
        console.log(config);
        const path = await invoke("open_dir_dialog");
        if (path === "" || path === null || path === undefined) {
            return;
        }
        setConfig((prev) => ({ ...prev, ["romfs"]: path }));
    }

    const onClose = () => {
        setIsOptionsOpen(false);
        setIsModalOpen(!isModalOpen);
        setStatusText("Options closed");
    };

    const isOpen = isOptionsOpen;

    useEffect(() => {
        if (isOpen) {
            fetchConfig();
        }
    }, [isOpen]);

    const fetchConfig = async () => {
        try {
            const configData = await invoke("get_toml_config");
            setConfig(configData);
            setConfigLoading(false);
            setStatusText("Options loaded");
        } catch (error) {
            console.error("Error fetching config:", error);
        }
    };

    const handleChange = (key, value) => {
        setConfig((prev) => ({ ...prev, [key]: value }));
    };

    const handleSave = async () => {
        let is_zstd_working = false;
        try {
            console.log("Saving config:", config);
            const content = await invoke("update_toml_config", { newConfig: config });
            onClose();
            if (content) {
                setStatusText(content.status_text);
                if (content.status_text.startsWith("ZSTD available")) {
                    is_zstd_working = true;
                }
            }
        } catch (error) {
            console.error("Error saving config:", error);
        }
        if (is_zstd_working || statusText.startsWith("ZSTD available")) {
            setSettings(prev => ({ ...prev, zstd_msg: "" }));
        }
        console.log("Settings ", settings);
        console.log("zstd_msg ", settings.zstd_msg);
        console.log("statusText ", statusText);
    };

    if (!isOpen) return null;

    // Define dropdown options for specific settings
    const dropdownOptions = {
        "Text editor theme": ["light", "dark", "vs-dark"],
        // "font size": [12, 14, 16, 18, 20],
        "rotation in degrees": [0, 90, 180, 270],
        "Byml inline container max count": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    };

    return (
        <div className="modal-overlay">
            <div className="modal-content">
                <h2>Options</h2>
                <div className="options-grid">
                    <div></div> <div
                        style={{ textAlign: "right" }}><button
                            className="modal-footer-button"
                            onClick={updateRomfsPath}
                            title="Select romfs path"
                        >
                            Select romfs path
                        </button></div>
                    {Object.entries(config).map(([key, value]) => (
                        <React.Fragment key={key}>
                            {/* Left Column: Key */}
                            <div className="config-label">
                                <label>{key}</label>
                            </div>

                            {/* Right Column: Value */}
                            <div className="config-value">
                                {typeof value === "boolean" ? (
                                    <input
                                        type="checkbox"
                                        checked={value}
                                        onChange={(e) => handleChange(key, e.target.checked)}
                                    />
                                ) : dropdownOptions[key] ? (
                                    <select
                                        value={value}
                                        onChange={(e) => handleChange(key, e.target.value)}
                                    >
                                        {dropdownOptions[key].map((option) => (
                                            <option key={option} value={option}>
                                                {option}
                                            </option>
                                        ))}
                                    </select>
                                ) : (
                                    <input
                                        type={typeof value === "number" ? "number" : "text"}
                                        value={value}
                                        onChange={(e) => handleChange(key, e.target.value)}
                                    />
                                )}
                            </div>
                        </React.Fragment>
                    ))}
                </div>
                <div className="options-modal-footer">
                    <button onClick={handleSave}>Save</button>
                    <button onClick={onClose}>Cancel</button>
                </div>
            </div>
        </div>


    );

}

export default OptionsEditor;
