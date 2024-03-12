import React, { useState } from 'react';
import DirectoryNode from './DirectoryNode';
import { extractFileClick, editInternalSarcFile, fetchAndSetEditorContent, saveAsFileClick, saveFileClick } from './ButtonClicks';

const fontsize = '15px';

const buildTree = (paths) => {
  const root = {};
  paths.paths.forEach((path) => {
    path.split('/').reduce((acc, name, index, arr) => {
      if (!acc[name]) {
        acc[name] = index === arr.length - 1 ? null : {};
      }
      return acc[name] || {};
    }, root);
  });
  return root;
};
//{ editorRef, updateEditorContent, setStatusText, activeTab, setActiveTab, setLabelTextDisplay, setpaths, selectedPath, changeModal }
const DirectoryTree = ({ onNodeSelect, sarcPaths , setStatusText}) => {
  const [selectedNode, setSelectedNode] = useState("");
  const [searchQuery, setSearchQuery] = useState("");
  const tree = buildTree(sarcPaths);
  // setStatusText("Directory Tree Loaded  ");
  const handleSelectNode = (fullPath) => {
    setSelectedNode(fullPath, "LE");
    onNodeSelect(fullPath, "LE");
  };

  const handleContextMenu = (fullPath) => {
    setSelectedNode(fullPath);
    console.log(`Context menu for: ${fullPath}`);
  };

  const filteredTree = (node, path = "") => {
    return Object.entries(node).reduce((acc, [key, value]) => {
      const fullPath = path ? `${path}/${key}` : key;
      if (value === null) { // It's a file
        if (fullPath.toLowerCase().includes(searchQuery.toLowerCase())) {
          acc[key] = value;
        }
      } else { // It's a directory
        const filtered = filteredTree(value, fullPath);
        if (Object.keys(filtered).length > 0 || fullPath.toLowerCase().includes(searchQuery.toLowerCase())) {
          acc[key] = filtered;
        }
      }
      return acc;
    }, {});
  };

  const renderTree = searchQuery ? filteredTree(tree) : tree;

  return (
    <>
      <ul className="directory-tree" style={{ listStyleType: 'none', fontSize: fontsize, marginBottom: '88px' }}>
        {Object.entries(renderTree).map(([key, value]) => (
          <DirectoryNode
            key={key}
            node={value}
            name={key}
            path=""
            onContextMenu={handleContextMenu}
            sarcPaths={sarcPaths}
            selected={selectedNode}
            onSelect={handleSelectNode}
          />
        ))}
      </ul>
      <div className='textsearch' style={{ padding: '10px' }}>
        <input
          type="text"
          placeholder="Search..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          style={{ width: '100%', padding: '5px' }}
        />
        <button onClick={() => setSearchQuery("")} style={{ width: '70px', padding: '5px' }}>Clear</button>
      </div>
    </>
  );
};

export default DirectoryTree;
