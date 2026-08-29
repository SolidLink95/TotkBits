import React, { useLayoutEffect, useRef, useState } from 'react';
import { invoke } from './DocumentState';
import { extractRootFolderClick, extractFolderClick, editInternalSarcFile, openBphclLeaf, removeBphclNodeClick, replaceInternalFileClick, removeInternalFileClick, addInternalFileToDir, extractFileClick, addEmptyByml,addFilesFromDirRecursively, expandNestedSarc, editNestedSarcFile, extractNestedSarcFile, mutateNestedArchive } from './ButtonClicks';
import { useEditorContext } from './StateManager';
import {compareInternalFileWithOVanila} from './Comparer';
import { clearRflMiis } from './Mii';

const dirOpened = `dir_opened.webp`;
const dirClosed = `dir_closed.webp`;
const fileIcon = `file.webp`;
const iconSize = '20px';

const buildNestedTree = (paths) => {
  const root = {};
  paths.forEach((innerPath) => innerPath.split('/').reduce((parent, part, index, parts) => {
    if (!(part in parent)) parent[part] = index === parts.length - 1 ? null : {};
    return parent[part] || {};
  }, root));
  return root;
};

const NestedDirectoryNode = ({ node, name, innerParent, outerPath, selected, onSelect }) => {
  const { settings, activeTab, setStatusText, setActiveTab, setLabelTextDisplay, updateEditorContent, paths, setpaths, setPathsFilters, treeExpandedNodes, setTreeExpandedNodes, setCompareData, setRenamePromptMessage, setIsAddPrompt, setIsModalOpen } = useEditorContext();
  const [contextMenu, setContextMenu] = useState({ visible: false, x: 0, y: 0 });
  const isFile = node === null;
  const innerPath = innerParent ? `${innerParent}/${name}` : name;
  const childChain = `${outerPath}::${innerPath}`;
  const expansionKey = `nested:${childChain}`;
  const isCollapsed = !treeExpandedNodes.has(expansionKey);
  const setExpanded = (expanded) => setTreeExpandedNodes((current) => {
    const next = new Set(current);
    if (expanded) next.add(expansionKey);
    else next.delete(expansionKey);
    return next;
  });
  const toggleExpanded = () => setExpanded(isCollapsed);
  const expandedArchive = paths.nested_paths?.[childChain];
  const identity = `nested:${outerPath}:${innerPath}`;
  const selectedStyle = selected === identity ? '#303030' : 'transparent';
  const select = (event) => { event.stopPropagation(); onSelect(innerPath, isFile, identity, false); };
  const open = async (event) => {
    event.stopPropagation();
    if (isFile) {
      if (expandedArchive) { toggleExpanded(); return; }
      const expanded = await expandNestedSarc(childChain, setStatusText, setpaths, setPathsFilters);
      if (expanded) setExpanded(true);
      else editNestedSarcFile(outerPath, innerPath, setStatusText, setActiveTab, setLabelTextDisplay, updateEditorContent);
    }
    else toggleExpanded();
  };
  const mutate = async (action, options = {}) => { setContextMenu({ visible: false, x: 0, y: 0 }); await mutateNestedArchive(outerPath, options.path ?? innerPath, action, setStatusText, setpaths, options); };
  const chooseFile = async () => await invoke('open_file_dialog');
  const chooseDir = async () => await invoke('open_dir_dialog');
  const rename = () => {
    setContextMenu({ visible: false, x: 0, y: 0 });
    setRenamePromptMessage({
      message: isFile ? 'Rename the internal archive file:' : 'Rename the internal archive directory:',
      path: name,
      nestedChain: outerPath,
      nestedPath: innerPath,
    });
    setIsAddPrompt(false);
    setIsModalOpen(true);
  };
  const isBars = paths.file_type === 'BARS';
  const actions = isFile ? [
    { label: 'Edit', method: () => { setContextMenu({ visible: false, x: 0, y: 0 }); editNestedSarcFile(outerPath, innerPath, setStatusText, setActiveTab, setLabelTextDisplay, updateEditorContent); }, icon: 'context_menu/edit.webp', shortcut: '', isRender: true },
    
    { label: 'Extract', method: () => { setContextMenu({ visible: false, x: 0, y: 0 }); extractNestedSarcFile(outerPath, innerPath, setStatusText); }, icon: 'context_menu/extract.webp', shortcut: '', isRender: true },
    { label: 'Replace', method: async () => { const sourcePath = await chooseFile(); if (sourcePath) await mutate('replace', { sourcePath }); }, icon: 'context_menu/replace.webp', shortcut: '', isRender: true },
    { label: 'Delete', method: async () => { if (window.confirm(`Delete ${innerPath}?`)) await mutate('delete'); }, icon: 'context_menu/remove.webp', shortcut: '', isRender: !isBars },
    { label: 'Rename', method: rename, icon: 'context_menu/rename.webp', shortcut: '', isRender: !isBars },
    { label: 'Compare', method: async () => {
      setContextMenu({ visible: false, x: 0, y: 0 });
      const content = await mutateNestedArchive(outerPath, innerPath, 'compare', setStatusText, setpaths);
      if (content?.tab === 'COMPARER') {
        const data = content.compare_data ?? {};
        const file1 = data.file1 ?? {};
        const file2 = data.file2 ?? {};
        const content1 = file1.text ?? '';
        const content2 = file2.text ?? '';
        setCompareData((previous) => ({
          ...previous,
          content1,
          content2,
          filepath1: file1.path?.full_path ?? '',
          filepath2: file2.path?.full_path ?? '',
          label1: file1.label || innerPath,
          label2: file2.label || 'Original',
          isInternal: true,
          isTiedToMonaco: false,
          isSmall: content1.length < 500000 && content2.length < 500000,
          lang: content.lang || 'yaml',
        }));
        setActiveTab('COMPARER');
      }
    }, icon: 'context_menu/compare.webp', shortcut: '', isRender: !isBars },
    { label: 'Copy path', method: () => { navigator.clipboard.writeText(innerPath); setStatusText('Copied to clipboard'); setContextMenu({ visible: false, x: 0, y: 0 }); }, icon: 'context_menu/copy.webp', shortcut: '', isRender: true },
    { label: 'Expand archive', method: async () => { setContextMenu({ visible: false, x: 0, y: 0 }); if (await expandNestedSarc(childChain, setStatusText, setpaths, setPathsFilters)) setExpanded(true); }, icon: 'dir_opened.webp', shortcut: '', isRender: true },
    { label: 'Close', method: () => setContextMenu({ visible: false, x: 0, y: 0 }), icon: 'context_menu/close.webp', shortcut: '', isRender: true },
  ] : [
    { label: 'Add file', method: async () => { const sourcePath = await chooseFile(); if (sourcePath) { const fileName = sourcePath.replace(/\\/g, '/').split('/').pop(); await mutate('add', { sourcePath, newPath: null, path: `${innerPath}/${fileName}` }); } }, icon: 'context_menu/add_file.webp', shortcut: '', isRender: true },
    { label: 'Add folder', method: async () => { const sourcePath = await chooseDir(); if (sourcePath) await mutate('add_dir', { sourcePath }); }, icon: 'context_menu/add_dir.webp', shortcut: '', isRender: true },
    { label: 'Extract', method: () => mutate('extract_folder'), icon: 'context_menu/extract.webp', shortcut: '', isRender: true },
    { label: 'New byml', method: () => mutate('new_byml'), icon: 'context_menu/byml.webp', shortcut: '', isRender: true },
    { label: 'Delete', method: async () => { if (window.confirm(`Delete ${innerPath} and its contents?`)) await mutate('delete'); }, icon: 'context_menu/remove.webp', shortcut: '', isRender: paths.file_type !== 'BARS' },
    { label: 'Rename', method: rename, icon: 'context_menu/rename.webp', shortcut: '', isRender: paths.file_type !== 'BARS' },
    { label: 'Close', method: () => setContextMenu({ visible: false, x: 0, y: 0 }), icon: 'context_menu/close.webp', shortcut: '', isRender: true },
  ];
  return <li onClick={select}>
    <div style={{ borderRadius: '5px', width: '95%', cursor: 'pointer', display: 'flex', alignItems: 'center', color: 'white', backgroundColor: selectedStyle }}
      onDoubleClick={open}
      onContextMenu={(event) => {
        event.preventDefault();
        event.stopPropagation();
        setContextMenu({ visible: true, x: event.clientX, y: event.clientY });
      }}>
      <img src={isFile ? fileIcon : isCollapsed ? dirClosed : dirOpened} alt={name}
        style={{ marginRight: '5px', width: iconSize, height: iconSize }}
        onClick={(event) => { event.stopPropagation(); if (!isFile) toggleExpanded(); }} />
      <span>{name}</span>
    </div>
    {isFile && !isCollapsed && expandedArchive?.length > 0 && <ul style={{ marginLeft: '40px', listStyleType: 'none', padding: 0 }}>
      {Object.entries(buildNestedTree(expandedArchive)).map(([childName, child]) => <NestedDirectoryNode key={childName}
        node={child} name={childName} innerParent="" outerPath={childChain} selected={selected} onSelect={onSelect} />)}
    </ul>}
    {!isFile && <div className={`node-children ${isCollapsed ? 'collapsed' : 'expanded'}`}>
      <ul style={{ marginLeft: '40px', listStyleType: 'none', padding: 0 }}>
        {Object.entries(node).map(([childName, child]) => <NestedDirectoryNode key={childName} node={child} name={childName}
          innerParent={innerPath} outerPath={outerPath} selected={selected} onSelect={onSelect} />)}
      </ul>
    </div>}
    {contextMenu.visible && <ContextMenu x={contextMenu.x} y={contextMenu.y}
      onClose={() => setContextMenu({ visible: false, x: 0, y: 0 })} actions={actions} settings={settings} />}
  </li>;
};

const ContextMenu = ({ x, y, onClose, actions, settings }) => {
  const menuRef = useRef(null);
  const [position, setPosition] = useState({ x, y });

  useLayoutEffect(() => {
    const bounds = menuRef.current?.getBoundingClientRect();
    if (!bounds) return;
    const margin = 8;
    setPosition({
      x: Math.max(margin, Math.min(x, window.innerWidth - bounds.width - margin)),
      y: Math.max(margin, Math.min(y, window.innerHeight - bounds.height - margin)),
    });
  }, [x, y, actions.length]);

  return (
    <ul
      ref={menuRef}
      className="context-menu"
      style={{
        fontSize: settings.contextMenuFontSize,
        position: 'fixed',
        top: position.y,
        left: position.x,
        listStyleType: 'none',
        padding: '6px',
        boxShadow: '0 2px 10px rgba(0,0,0,0.2)',
        zIndex: 100,
      }}
      onMouseLeave={onClose}
    >
      {actions.map((action, index) => {
        if (!action.isRender) return null;
        return <li
          key={index}
          className="context-menu-item"
          onClick={() => action.method()}
          style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}
        >
          <div style={{ display: 'flex', alignItems: 'center' }}>
            <img src={action.icon} alt={action.label} style={{ marginRight: '10px', width: '20px', height: '20px' }} />
            {action.label}
          </div>
          <span style={{ marginLeft: '10px', color: '#bcbcbc' }}>{action.shortcut} </span>
        </li>;
      })}
    </ul>
  );
};
//{ editorRef, updateEditorContent, setStatusText, activeTab, setActiveTab, setLabelTextDisplay, setpaths, selectedPath, changeModal }
const DirectoryNode = ({ node, name, path, onContextMenu, sarcPaths, selected, onSelect, isArchiveRoot = false }) => {
  const {
    settings, setSettings,
    renamePromptMessage, setRenamePromptMessage,
    isAddPrompt, setIsAddPrompt,
    activeTab, setActiveTab,
    editorContainerRef, editorRef, editorValue, setEditorValue, lang, setLang,
    statusText, setStatusText, selectedPath, setSelectedPath, labelTextDisplay, setLabelTextDisplay,
    paths, setpaths, setPathsFilters, treeExpandedNodes, setTreeExpandedNodes,
    isModalOpen, setIsModalOpen, updateEditorContent, changeModal, setCompareData, setInternalSarcPath, setReadOnly
  } = useEditorContext();

  const [contextMenu, setContextMenu] = useState({ visible: false, x: 0, y: 0 });
  const isFile = node === null;
  const fullPath = isArchiveRoot ? '' : path ? `${path}/${name}` : name;
  const expansionKey = `root:${isArchiveRoot ? name : fullPath}`;
  const isCollapsed = !treeExpandedNodes.has(expansionKey);
  const setExpanded = (expanded) => setTreeExpandedNodes((current) => {
    const next = new Set(current);
    if (expanded) next.add(expansionKey);
    else next.delete(expansionKey);
    return next;
  });
  const toggleExpanded = () => setExpanded(isCollapsed);
  // const endian = "LE";
  const isSelected = selected === fullPath;

  const handleDoubleClick = async (e) => {
    e.stopPropagation(); // Prevent the click from bubbling up to parent elements
    console.log(`Double-clicked on directory: ${fullPath}`);
    if (isFile) {
      if (sarcPaths.nested_paths?.[fullPath]) {
        toggleExpanded();
        return;
      }
      const expanded = await expandNestedSarc(fullPath, setStatusText, setpaths, setPathsFilters);
      if (expanded) setExpanded(true);
      else handleOpenInternalSarcFile();
    } else {
      toggleCollapse();
    }
    // Add your custom double-click logic here
  };

  const handleSelect = (e) => {
    e.stopPropagation(); // This stops the event from bubbling up further
    console.log(`Selected: ${fullPath}`);
    onSelect(fullPath, isFile); // Pass the fullPath to the onSelect function
  };

  const handleAddClick = () => {
    closeContextMenu();
    setIsAddPrompt(true);
    setIsModalOpen(true);
  }

  const handleExtractInternalSarcFile = () => {
    closeContextMenu();
    if (isFile) {
      extractFileClick({ path: fullPath }, setStatusText);
    }
  };
  const handleExtractInternalSarcFolder = () => {
    closeContextMenu();
    if (!isFile) {
      console.log("Extracting folder:", fullPath);
      extractFolderClick(fullPath, setStatusText);
    }
  };

  const handleOpenInternalSarcFile = () => {
    closeContextMenu();
    if (isFile) {
      if (/\.(?:bfwav|bwav)$/i.test(fullPath)) {
        setStatusText('Decoding audio…');
        invoke('open_bfwav_node', { path: fullPath }).then((preview) => {
          window.dispatchEvent(new CustomEvent('totkbits:audio-preview', { detail: preview }));
          setActiveTab('AUDIO');
          setStatusText(`Opened audio: ${fullPath}`);
        }).catch((error) => setStatusText(`Audio error: ${error}`));
        return;
      }
      if (/\.amta$/i.test(fullPath)) {
        setStatusText('Parsing AMTA metadata…');
        invoke('open_amta_node', { path: fullPath }).then((content) => {
          if (!content) return;
          setLabelTextDisplay((previous) => ({ ...previous, yaml: content.file_label }));
          updateEditorContent(content.text, 'yaml');
          setReadOnly(true);
          setActiveTab('YAML');
          setStatusText(content.status_text);
        }).catch((error) => setStatusText(`AMTA error: ${error}`));
        return;
      }
      if (sarcPaths.read_only) openBphclLeaf(fullPath, sarcPaths.documentId, setStatusText, setActiveTab, setLabelTextDisplay, updateEditorContent, setReadOnly);
      else editInternalSarcFile(fullPath, setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);
    }
  };
  const handleCompareInternalSarcFile = () => {
    closeContextMenu();
    if (isFile) {
      compareInternalFileWithOVanila(selectedPath.path, setStatusText, setActiveTab, setCompareData);
    }
  };
  const handleRemoveInternalSarcFile = () => {
    closeContextMenu();
    removeInternalFileClick(fullPath, setStatusText, setpaths);
  };
  const handleClearRflDb = async () => {
    closeContextMenu();
    await clearRflMiis(setStatusText, setpaths);
  };
  const handleRemoveBphclNode = async () => {
    closeContextMenu();
    if (window.confirm(`Delete ${name}?`)) {
      await removeBphclNodeClick(fullPath, setStatusText, setpaths);
    }
  };
  const handleReplaceInternalSarcFile = () => {
    closeContextMenu();
    if (/\.(?:bfwav|bwav)$/i.test(fullPath)) {
      invoke('open_audio_file_dialog').then(async (sourcePath) => {
        if (!sourcePath) return;
        try {
          window.dispatchEvent(new CustomEvent('totkbits:audio-processing', { detail: 'Encoding replacement audio…' }));
          setStatusText('Encoding replacement audio…');
          await invoke('replace_bfwav_node', { path: fullPath, sourcePath });
          setpaths((current) => ({ ...current, modded_paths: [...new Set([...(current.modded_paths || []), fullPath])] }));
          const preview = await invoke('open_bfwav_node', { path: fullPath });
          window.dispatchEvent(new CustomEvent('totkbits:audio-preview', { detail: preview }));
          setActiveTab('AUDIO');
          setStatusText(`Replaced ${fullPath}`);
        } catch (error) { setStatusText(`Audio replacement failed: ${error}`); }
        finally { window.dispatchEvent(new CustomEvent('totkbits:audio-processing')); }
      });
      return;
    }
    replaceInternalFileClick(fullPath, setStatusText, setpaths, sarcPaths.file_type);
  };
  const handleAddInternalSarcFileToDir = () => {
    closeContextMenu();
    addInternalFileToDir(fullPath, setStatusText, setpaths, sarcPaths.file_type);
  };  
  const handleAddFilesFromDirRecursively = () => {
    closeContextMenu();
    addFilesFromDirRecursively(fullPath, setStatusText, setpaths);
  };
  const handleAddEmptyByml = () => {
    closeContextMenu();
    addEmptyByml(fullPath, setStatusText, setpaths);
  };

  const handleReplaceBarsFromFolder = async () => {
    closeContextMenu();
    const folderPath = await invoke('open_dir_dialog');
    if (!folderPath) return;
    setStatusText('Matching and encoding audio replacements…');
    window.dispatchEvent(new CustomEvent('totkbits:audio-processing', { detail: 'Replacing BARS audio from folder…' }));
    try {
      const result = await invoke('replace_bars_audio_from_folder', { folderPath });
      setpaths((current) => ({ ...current, modded_paths: [...new Set([...(current.modded_paths || []), ...result.replaced])] }));
      const failure = result.failed.length ? `; ${result.failed.length} failed` : '';
      setStatusText(`Replaced ${result.replaced.length} matching audio files; ${result.skipped.length} unmatched${failure}`);
      if (result.failed.length) window.alert(result.failed.join('\n'));
    } catch (error) {
      setStatusText(`Folder replacement failed: ${error}`);
    } finally {
      window.dispatchEvent(new CustomEvent('totkbits:audio-processing'));
    }
  };

  const handleRenameInternalSarcFile = () => {
    closeContextMenu();
    if (isFile) {
      setRenamePromptMessage({ message: "Rename the internal SARC file:", path: name });
    } else {
      setRenamePromptMessage({ message: "Rename the internal SARC directory:", path: name });
    }
    setIsAddPrompt(false);
    setIsModalOpen(true);

  }
  const handlePathToClipboard = (text) => {
    closeContextMenu();
    navigator.clipboard.writeText(text).then(() => {
      console.log('Text copied to clipboard');
    }).catch(err => {
      console.error('Failed to copy text: ', err);
    });
    setStatusText(`Copied to clipboard`);
  }


  const nodeStyle = {
    borderRadius: '5px',
    width: '95%',
    cursor: 'pointer',
    display: 'flex',
    alignItems: 'center',
    color: isSelected && isFile ? 'white' : 'white',
    backgroundColor: //isFile ?
      isSelected ?
        sarcPaths.added_paths.includes(fullPath) ? '#2D8589' :
          sarcPaths.modded_paths.includes(fullPath) ? '#B78F00' :
            '#303030' :
        sarcPaths.added_paths.includes(fullPath) ? '#1E595B' :
          sarcPaths.modded_paths.includes(fullPath) ? '#826C00' :
            'transparent'// :
    // 'transparent'


  };

  const toggleCollapse = () => {
    toggleExpanded();
    closeContextMenu();
  };

  const handleIconClick = (e) => {
    if (!isFile) {
      toggleCollapse();
    }
    e.stopPropagation();
  };

  const handleIconContextMenu = (e) => {
    e.preventDefault();
    e.stopPropagation();
    setContextMenu({
      visible: true,
      x: e.clientX,
      y: e.clientY,
    });
    onContextMenu && onContextMenu(fullPath);
  };

  const closeContextMenu = () => {
    setContextMenu({ visible: false, x: 0, y: 0 });
  };

  const isBars = sarcPaths.file_type === 'BARS';
  const readOnlyPhysicsNode = sarcPaths.read_only && activeTab === 'SARC';
  const readOnlyActions = [
    { label: 'View', method: handleOpenInternalSarcFile, icon: 'context_menu/edit.webp', shortcut: '', isRender: true },
    { label: 'Extract', method: handleExtractInternalSarcFile, icon: 'context_menu/extract.webp', shortcut: '', isRender: true },
    { label: 'Delete', method: handleRemoveBphclNode, icon: 'context_menu/remove.webp', shortcut: '', isRender: activeTab === 'SARC' && !isBars },
    { label: 'Copy path', method: () => handlePathToClipboard(fullPath), icon: 'context_menu/copy.webp', shortcut: '', isRender: true },
    { label: 'Close', method: () => closeContextMenu(), icon: 'context_menu/close.webp', shortcut: '', isRender: true },
  ];
  const contextMenuActions = isArchiveRoot ? [
    { label: 'Clear all', method: handleClearRflDb, icon: 'context_menu/remove.webp', shortcut: '', isRender: sarcPaths.file_type === 'RFL_DB' && sarcPaths.paths.length > 0 },
    { label: 'Close', method: () => closeContextMenu(), icon: 'context_menu/close.webp', shortcut: '', isRender: true },
  ] : readOnlyPhysicsNode && isFile ? readOnlyActions : isFile ? [
    { label: 'Edit', method: handleOpenInternalSarcFile, icon: 'context_menu/edit.webp', shortcut: '', isRender: true },
    { label: 'Compare', method: handleCompareInternalSarcFile, icon: 'context_menu/compare.webp', shortcut: '', isRender: !isBars },
    { label: 'Extract', method: handleExtractInternalSarcFile, icon: 'context_menu/extract.webp', shortcut: '', isRender: true },
    { label: 'Replace', method: handleReplaceInternalSarcFile, icon: 'context_menu/replace.webp', shortcut: '', isRender: true },
    { label: 'Delete', method: handleRemoveInternalSarcFile, icon: 'context_menu/remove.webp', shortcut: '', isRender: !isBars },
    { label: 'Rename', method: handleRenameInternalSarcFile, icon: 'context_menu/rename.webp', shortcut: '', isRender: !isBars },
    { label: 'Copy path', method: () => handlePathToClipboard(fullPath), icon: 'context_menu/copy.webp', shortcut: '', isRender: true },
    { label: 'Expand archive', method: async () => { closeContextMenu(); if (await expandNestedSarc(fullPath, setStatusText, setpaths, setPathsFilters)) setExpanded(true); }, icon: 'dir_opened.webp', shortcut: '', isRender: !isBars },
    { label: 'Close', method: () => closeContextMenu(), icon: 'context_menu/close.webp', shortcut: '', isRender: true },
  ] : [
    { label: 'Replace audio from folder', method: handleReplaceBarsFromFolder, icon: 'context_menu/replace.webp', shortcut: '', isRender: sarcPaths.file_type === 'BARS' },
    { label: 'Add file', method: handleAddInternalSarcFileToDir, icon: 'context_menu/add_file.webp', shortcut: '', isRender: true },
    { label: 'Add folder', method: handleAddFilesFromDirRecursively, icon: 'context_menu/add_dir.webp', shortcut: '', isRender: true },
    { label: 'Extract', method: handleExtractInternalSarcFolder, icon: 'context_menu/extract.webp', shortcut: '', isRender: true },
    { label: 'New byml', method: handleAddEmptyByml, icon: 'context_menu/byml.webp', shortcut: '', isRender: true },
    { label: 'Delete', method: handleRemoveInternalSarcFile, icon: 'context_menu/remove.webp', shortcut: '', isRender: !isBars },
    { label: 'Rename', method: handleRenameInternalSarcFile, icon: 'context_menu/rename.webp', shortcut: '', isRender: !isBars },
    { label: 'Close', method: () => closeContextMenu(), icon: 'context_menu/close.webp', shortcut: '', isRender: true },
  ];

  return (
    <li onClick={handleSelect}>
      <div style={nodeStyle}
        onContextMenu={handleIconContextMenu}
        onDoubleClick={handleDoubleClick}>
        <img
          src={isFile ? fileIcon : isCollapsed ? dirClosed : dirOpened}
          alt={name}
          style={{ marginRight: '5px', width: iconSize, height: iconSize }}
          onClick={handleIconClick}
          onContextMenu={handleIconContextMenu}
        />
        <span onClick={toggleCollapse}>{name}</span>
      </div>
      {isFile && !isCollapsed && sarcPaths.nested_paths?.[fullPath]?.length > 0 && (
        <ul style={{ marginLeft: '40px', listStyleType: 'none', padding: 0 }}>
          {Object.entries(buildNestedTree(sarcPaths.nested_paths[fullPath])).map(([nestedName, nestedNode]) => (
            <NestedDirectoryNode key={nestedName} node={nestedNode} name={nestedName} innerParent=""
              outerPath={fullPath} selected={selected} onSelect={onSelect} />
          ))}
        </ul>
      )}
      {!isFile && (
        <div className={`node-children ${isCollapsed ? 'collapsed' : 'expanded'}`}>
          <ul style={{ marginLeft: '40px', listStyleType: 'none', padding: 0 }}>
            {Object.entries(node).map(([key, value]) => (
              <DirectoryNode
                key={key}
                node={value}
                name={key}
                path={isArchiveRoot ? '' : fullPath}
                onContextMenu={onContextMenu}
                sarcPaths={sarcPaths}
                selected={selected} // Make sure this is passed correctly
                onSelect={onSelect} // Make sure this is passed correctly
              />
            ))}
          </ul>
        </div>
      )}
      {contextMenu.visible && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={closeContextMenu}
          actions={contextMenuActions}
          settings={settings}
        />
      )}
    </li>
  );
};


export default DirectoryNode;
