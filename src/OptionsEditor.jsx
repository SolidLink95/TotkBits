import React, { useEffect } from "react";
import * as monaco from "monaco-editor";
import { invoke } from './DocumentState';
import { useEditorContext } from "./StateManager";

const fields = [
    { key: "romfs", label: "TOTK RomFS path", type: "path" },
    { key: "Stop asking for romfs path", label: "Stop asking for a RomFS path", type: "boolean" },
    { key: "BOTW WIIU path (optional)", label: "BOTW Wii U path", type: "path" },
    { key: "AOC path (optional)", label: "Age of Calamity path", type: "path" },
    { key: "Tomodachi path (optional)", label: "Tomodachi Life path", type: "path" },
    { key: "LM3 path (optional)", label: "Luigi's Mansion 3 RomFS path", type: "path" },
    { key: "UI scale", label: "Entire UI scale", type: "number", min: 0.2, max: 3.0, step: 0.1 },
    { key: "font size", label: "Editor font size", type: "number", min: 8, max: 72 },
    { key: "Context menu font size", label: "Context-menu font size", type: "number", min: 8, max: 40 },
    { key: "Byml inline container max count", label: "BYML inline item limit", type: "number", min: 1, max: 10 },
    { key: "Text editor theme", label: "Editor theme", type: "select", options: ["vs", "vs-dark", "hc-black", "hc-light"] },
    { key: "Lower float precision", label: "Lower float precision", type: "boolean" },
    { key: "Text editor minimap", label: "Show editor minimap", type: "boolean" },
    { key: "Prompt on close all", label: "Prompt before closing all", type: "boolean" },
    { key: "Rotation in degrees", label: "Display rotations in degrees", type: "boolean" },
    { key: "ask for compression", label: "Ask for compression", type: "boolean" },
    { key: "rstb", label: "RSTB view", type: "select", options: ["editor", "json"] },
    { key: "xlink_format", label: "XLink format", type: "select", options: ["legacy", "modern"] },
];

function OptionsEditor() {
    const {
        settings, setSettings, setStatusText, setIsOptionsOpen, setConfig,
        setConfigLoading, isOptionsOpen, config, configLoading, setIsModalOpen,
        editorRef, rightEditorRef,
    } = useEditorContext();

    useEffect(() => {
        if (!isOptionsOpen) return;
        setConfigLoading(true);
        invoke("get_toml_config")
            .then((configData) => {
                setConfig(configData);
                setStatusText("Settings loaded");
            })
            .catch((error) => {
                console.error("Error fetching config:", error);
                setStatusText(`Error: unable to load settings: ${error}`);
            })
            .finally(() => setConfigLoading(false));
    }, [isOptionsOpen, setConfig, setConfigLoading, setStatusText]);

    const close = () => {
        setIsOptionsOpen(false);
        setIsModalOpen(false);
        setStatusText("Settings closed");
    };

    const chooseDirectory = async (key) => {
        const path = await invoke("open_dir_dialog");
        if (path) setConfig((current) => ({ ...current, [key]: path }));
    };

    const revealTomlConfig = async () => {
        try {
            await invoke("edit_config");
            setStatusText("Config file shown in Explorer");
        } catch (error) {
            console.error("Error revealing config file:", error);
            setStatusText(`Error: unable to show config file: ${error}`);
        }
    };

    const change = (field, rawValue) => {
        const value = field.type === "number" ? Number(rawValue) : rawValue;
        setConfig((current) => ({ ...current, [field.key]: value }));
    };

    const save = async () => {
        try {
            const uiScale = Math.min(3, Math.max(0.2, Number(config["UI scale"]) || 1));
            const normalizedConfig = { ...config, "UI scale": uiScale };
            const content = await invoke("update_toml_config", { newConfig: normalizedConfig });
            setConfig(normalizedConfig);
            setSettings((current) => ({
                ...current,
                uiScale,
                fontSize: config["font size"],
                contextMenuFontSize: config["Context menu font size"],
                theme: config["Text editor theme"],
                minimap: config["Text editor minimap"],
                zstd_msg: content?.status_text?.includes("dictionaries are available")
                    ? ""
                    : content?.status_text || current.zstd_msg,
            }));
            monaco.editor.setTheme(config["Text editor theme"]);
            const editorOptions = {
                fontSize: config["font size"],
                minimap: { enabled: config["Text editor minimap"] },
            };
            editorRef.current?.updateOptions(editorOptions);
            rightEditorRef.current?.updateOptions(editorOptions);
            setIsOptionsOpen(false);
            setIsModalOpen(false);
            window.dispatchEvent(new CustomEvent('totkbits:aoc-config-changed'));
            setStatusText(content?.status_text || "Settings saved");
        } catch (error) {
            console.error("Error saving config:", error);
            setStatusText(`Error: unable to save settings: ${error}`);
        }
    };

    if (!isOptionsOpen) return null;

    return (
        <div className="modal-overlay" role="dialog" aria-modal="true" aria-labelledby="settings-title">
            <div className="modal-content settings-modal">
                <div className="settings-header">
                    <h2 id="settings-title">Settings</h2>
                    <button className="settings-close" onClick={close} aria-label="Close settings" title="Close">×</button>
                </div>
                {configLoading ? <div className="settings-loading">Loading settings…</div> : (
                    <div className="options-grid">
                        {fields.map((field) => (
                            <React.Fragment key={field.key}>
                                <label className="config-label" htmlFor={`setting-${field.key}`}>{field.label}</label>
                                <div className="config-value settings-control">
                                    {field.type === "boolean" ? (
                                        <input id={`setting-${field.key}`} type="checkbox" checked={Boolean(config[field.key])}
                                            onChange={(event) => change(field, event.target.checked)} />
                                    ) : field.type === "select" ? (
                                        <select id={`setting-${field.key}`} value={config[field.key] ?? ""}
                                            onChange={(event) => change(field, event.target.value)}>
                                            {field.options.map((option) => <option key={option} value={option}>{option}</option>)}
                                        </select>
                                    ) : (
                                        <>
                                            <input id={`setting-${field.key}`} type={field.type === "number" ? "number" : "text"}
                                                min={field.min} max={field.max} required={field.required}
                                                step={field.step}
                                                value={config[field.key] ?? ""} onChange={(event) => change(field, event.target.value)} />
                                            {field.type === "path" && <button type="button" onClick={() => chooseDirectory(field.key)}>Browse…</button>}
                                        </>
                                    )}
                                </div>
                            </React.Fragment>
                        ))}
                    </div>
                )}
                <details className="settings-advanced">
                    <summary>Advanced</summary>
                    <button type="button" onClick={revealTomlConfig}>Edit TOML</button>
                </details>
                <div className="options-modal-footer">
                    <button onClick={save} disabled={configLoading}>Save</button>
                    <button onClick={close}>Close</button>
                </div>
            </div>
        </div>
    );
}

export default OptionsEditor;
