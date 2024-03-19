import { invoke } from '@tauri-apps/api/tauri'; // Import Tauri invoke method
import React, { useState } from 'react';

const marg = '10px';
const button_size = '33px';
const max_entries = 2000;

const RstbTree = ({ onNodeSelect, sarcPaths, setStatusText, activeTab }) => {
  const [searchQuery, setSearchQuery] = useState("");
  const [searchVal, setSearchVal] = useState("");
  const [entries, setEntries] = useState([]); // New state to hold filtered data
  // const [filteredData, setFilteredData] = useState([]); // New state to hold filtered data
  const [selectedNode, setSelectedNode] = useState(null);


  function ImageButton({ src, onClick, alt, title, style }) {
    // Apply both the background image and styles directly to the button
    return (
      <button
        onClick={onClick}
        className='button'
        style={{
          backgroundImage: `url(${src})`,
          backgroundSize: 'cover', // Cover the entire area of the button
          backgroundPosition: 'center', // Center the background image
          minHeight: button_size, // Define your desired width
          minWidth: button_size, // Define your desired width
          width: button_size, // Define your desired width
          height: button_size, // Define your desired height 
          display: 'flex', // Ensure the button content (if any) is centered
          justifyContent: 'left', // Center horizontally
          alignItems: 'left', // Center vertically
          ...style // Spread additional styles here
        }}
        aria-label={alt} // Accessibility label for the button if the image fails to load or for screen readers
        title={title}
      >
      </button>
    );
  }

  // const data = [
  //   { path: "gallery", val: "150" },
  //   { path: "fantasy", val: "205" },
  //   { path: "dolphin", val: "260" },
  //   { path: "library", val: "315" },
  //   { path: "mystery", val: "370" },
  //   { path: "journey", val: "425" },
  //   { path: "kingdom", val: "480" },
  //   { path: "quantum", val: "535" },
  //   { path: "zealous", val: "590" },
  //   { path: "victory", val: "645" },
  //   { path: "utility", val: "700" },
  //   { path: "voyager", val: "755" },
  //   { path: "uncover", val: "810" },
  //   { path: "trilogy", val: "865" },
  //   { path: "soprano", val: "920" },
  //   { path: "reality", val: "975" },
  //   { path: "quality", val: "230" },
  //   { path: "pursuit", val: "285" },
  //   { path: "organic", val: "340" },
  //   { path: "nectar", val: "395" },
  // ];

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

  const handleSearch = async () => {
    if (searchQuery.length < 3) {
      setStatusText("Search query must be at least 3 characters");
      return;
    }
    // Filter data based on searchQuery when the "Search" button is clicked
    // setFilteredData(data.filter(node => node.path.includes(searchQuery) || node.val.includes(searchQuery)));
    try {
      setStatusText("Searching...");
      const content = await invoke('rstb_get_entries', { entry: searchQuery });
      if (content !== null) {
        if (content.rstb_paths.length > max_entries) {
          setStatusText(`Found ${content.rstb_paths.length} entries (more than ${max_entries}), please refine your search`);
          return;
        }
        setEntries(content.rstb_paths);
        console.log(content.rstb_paths);
        setStatusText(content.status_text);
      } else {
        setStatusText("Error: is any RSTB file opened?");
      }
    } catch (error) {
      console.error('Failed to search:', error);
    }
  };

  const handleClear = () => {
    setStatusText("Cleared RSTB search results"); // Clear search query
    setSearchQuery(""); // Clear search query
    setSearchVal(""); // Clear search query
    setEntries([]); // Clear filtered data
  };

  const handleRemoveEntry = async (node) => {
    try {
      setStatusText("Removing...");
      const content = await invoke('rstb_remove_entry', { entry: node.path });
      if (content === null) {
        setStatusText("Error: is any RSTB file opened?");
        return;
      }
      setStatusText(content.status_text);
      const newEntries = entries.filter((entry) => entry.path !== node.path);
      setEntries(newEntries);
    } catch (error) {
      console.error('Failed to remove:', error);
    }
  }

  const handleSave = async () => {
    try {
      if (searchQuery.length === 0) {
        setStatusText("Search query must not be empty!");
        return;
      }
      if (!Number.isInteger(parseInt(searchVal))) {
        setStatusText("RSTB value must be an integer!");
        return;
      }
      setStatusText("Saving...");
      const content = await invoke('rstb_edit_entry', { entry: searchQuery, val: searchVal });
      if (content === null) {
        setStatusText("Error: is any RSTB file opened?");
        return;
      }
      setStatusText(content.status_text);
    } catch (error) {
      // console.error('Failed to save:', error);
      setStatusText(`Failed to save: ${error}`);
    }
  };

  return (
    <div style={{ display: activeTab === 'RSTB' ? "flex" : "none", }}>
      <div className='textsearch' style={{ padding: '10px', }}>
        {/* <button onClick={handleSearch} style={{ marginRight: marg }}>Search</button> */}
        <ImageButton src="lupa.png" onClick={handleSearch} alt="Search" title="Search" style={{ marginRight: marg }} />
        <input
          type="text"
          placeholder="Type at least 3 characters"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          style={{ width: 'calc(100% - 90px)', padding: '5px', color: 'white', }}
        />
        <input
          type="text"
          placeholder="value"
          value={searchVal}
          onChange={(e) => setSearchVal(e.target.value)}
          style={{ minWidth: "50px", width: '10%', padding: '5px', color: 'white', marginLeft: marg }}
        />
        {/* <button onClick={handleClear} style={{ marginLeft: marg }}>Save</button> */}
        <ImageButton src="save_rstb.png" onClick={handleSave} alt="Save" title="Save entry" style={{ marginLeft: marg }} />
        {/* <button onClick={handleClear} style={{ marginLeft: marg }}>Clear</button> */}
        <ImageButton src="clear_rstb.png" onClick={handleClear} alt="Clear" title="Clear search" style={{ marginLeft: "1px" }} />
      </div>

      <div className='rstb-tree' >
        {entries.map((node) => (
          <div className='rstb-node' key={node.path} onClick={() => handleNodeSelect(node)} style={{
            background: selectedNode === node.path ? '#3a3a3a' : '#444444'
          }}>
            <span style={{ flexGrow: 1, textAlign: 'left', maxWidth: '6000px', overflow: "hidden", textOverflow: "ellipsis" }}>{node.path}</span>
            <span style={{ marginLeft: '20px', marginRight: 'auto', textAlign: 'right' }}>{node.val}</span>
            {/* <button onClick={() => handleEditRow(node)} style={{ marginLeft: marg }}>Edit</button> */}
            <ImageButton src="edit_rstb.png" onClick={() => handleEditRow(node)} alt="Edit" title="Edit" style={{ marginLeft: marg }} />
            <ImageButton src="remove.png" onClick={() => handleRemoveEntry(node)} alt="Remove" title="Remove"  />
            {/* <button onClick={(e) => e.stopPropagation()} style={{ marginLeft: marg }}>Remove</button> */}
          </div>
        ))}
      </div>
    </div>
  );
};

export default RstbTree;