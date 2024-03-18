import React, { useState } from 'react';
import DirectoryNode from './DirectoryNode';
import { extractFileClick, editInternalSarcFile, fetchAndSetEditorContent, saveAsFileClick, saveFileClick } from './ButtonClicks';

const marg = '5px';

const RstbTree = ({ onNodeSelect, sarcPaths, setStatusText, activeTab }) => {
  const [searchQuery, setSearchQuery] = useState("");
  const [searchVal, setSearchVal] = useState("");
  const [filteredData, setFilteredData] = useState([]); // New state to hold filtered data
  const [selectedNode, setSelectedNode] = useState(null);

  const data = [
    { path: "gallery", val: "150" },
    { path: "fantasy", val: "205" },
    { path: "dolphin", val: "260" },
    { path: "library", val: "315" },
    { path: "mystery", val: "370" },
    { path: "journey", val: "425" },
    { path: "kingdom", val: "480" },
    { path: "quantum", val: "535" },
    { path: "zealous", val: "590" },
    { path: "victory", val: "645" },
    { path: "utility", val: "700" },
    { path: "voyager", val: "755" },
    { path: "uncover", val: "810" },
    { path: "trilogy", val: "865" },
    { path: "soprano", val: "920" },
    { path: "reality", val: "975" },
    { path: "quality", val: "230" },
    { path: "pursuit", val: "285" },
    { path: "organic", val: "340" },
    { path: "nectar", val: "395" },
  ];
  
  const handleNodeSelect = (node) => {
    setSelectedNode(node.path);
    // setSearchQuery(node.path);
    // setSearchVal(node.val);
  };

  const handleEditRow = (node) => {
    setSelectedNode(node.path);
    setSearchQuery(node.path);
    setSearchVal(node.val);
  };

  const logPath = (path, e) => {
    e.stopPropagation(); // Prevents the onClick event of the parent div from being called
    console.log(path);
  };

  const handleSearch = () => {
    // Filter data based on searchQuery when the "Search" button is clicked
    setFilteredData(data.filter(node => node.path.includes(searchQuery) || node.val.includes(searchQuery)));
  };

  const handleClear = () => {
    setSearchQuery(""); // Clear search query
    setFilteredData([]); // Clear filtered data
  };

  return (
    <div style={{ display: activeTab === 'RSTB' ? "flex" : "none",  }}>
      <div className='textsearch' style={{ padding: '10px', }}>
        <button onClick={handleSearch} style={{ marginRight: marg }}>Search</button>
        <input
          type="text"
          placeholder="Type at least 3 characters"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          style={{ width: 'calc(100% - 90px)', padding: '5px', color: 'white',  }}
        />
        <input
          type="text"
          placeholder="value"
          value={searchVal}
          onChange={(e) => setSearchVal(e.target.value)}
          style={{ width: '20%', padding: '5px', color: 'white',  marginLeft: marg }}
        />
        <button onClick={handleClear} style={{ marginLeft: marg }}>Save</button>
        <button onClick={handleClear} style={{ marginLeft: marg }}>Clear</button>
      </div>

      <div className='rstb-tree' >
        {filteredData.map((node) => (
          <div className='rstb-node' key={node.path} onClick={() => handleNodeSelect(node)} style={{
            background: selectedNode === node.path ? '#3a3a3a' : '#444444'}}>
            <span style={{ flexGrow: 1, textAlign: 'left' }}>{node.path}</span>
            <span style={{ marginRight: 'auto', textAlign: 'right' }}>{node.val}</span>
            <button onClick={() => handleEditRow(node)} style={{ marginLeft: marg }}>Edit</button>
            <button onClick={(e) => e.stopPropagation()} style={{ marginLeft: marg }}>Remove</button>
          </div>
        ))}
      </div>
    </div>
  );
};

export default RstbTree;