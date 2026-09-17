import { discardActiveComparisonDocument, invoke } from './DocumentState';
import React, { useEffect, useRef, useState } from 'react';
import { useEditorContext } from './StateManager';

import { DiffEditor } from '@monaco-editor/react';

const bigFileSize = 1 * 1024 * 1024;
const MAX_COMPARE_SIZE = 9999 * 1024 * 1024;

export function clearCompareData(setCompareData) {
  setCompareData((prevData) => ({
    ...prevData,
    content1: '',
    content2: '',
    filepath1: '',
    filepath2: '',
    isInternal: false,
    label1: '',
    label2: '',
    isSmall: true,
    isTiedToMonaco: false,
    lang: 'yaml'
  }));
}

const handleCompare = async (event) => {
  event.stopPropagation(); // Prevent click event from reaching parent
  closeMenu();
  try {
    const content = await invoke('extract_folder_from_opened_sarc', { sourceFolder: "" });
    console.log(content);
    if (content !== null && content.status_text !== undefined) {
      setStatusText(content.status_text);
    }
  } catch (error) {
    console.error('Failed to extract sarc: ', error);
  }
}

export async function compareInternalFileWithOVanila(internalPath, setStatusText, setActiveTab, setCompareData) {
  try {
    const isFromSarc = true;
    const isFromMonaco = !isFromSarc;
    const path = internalPath ?? '';
    if (path === '') {
      setStatusText('Select some file to compare first!');
      return;
    }
    const content = await invoke('compare_internal_file_with_vanila', { internalPath: path, isFromSarc: isFromSarc });
    if (!content) {
      console.log("No content returned from compare_internal_file_with_vanila");
      return;
    }
    setStatusText(content.status_text);
    if (content.status_text.toLowerCase().startsWith('error:')) {
      console.error(content.status_text);
      return;
    }
    //It worked, asign the data to the compareData
    const data = content.compare_data;
    const text1 = data.file1.text ?? '';
    const text2 = data.file2.text ?? '';
    if (text1 === text2) {
      await discardActiveComparisonDocument();
      setStatusText('Files are identical! Skipping comparison.');
      return;
    }
    const full_path1 = data.file1.path.full_path.replace(/\\/g, '/');
    const full_path2 = data.file2.path.full_path.replace(/\\/g, '/');
    let label1 = data.file1.label;
    label1 = label1.length > 0 ? label1 : full_path1;
    let label2 = data.file2.label;
    label2 = label2.length > 0 ? label2 : full_path2;
    const lang = content.lang ?? 'yaml';

    setCompareData((prevData) => ({
      ...prevData,
      content1: text1,
      content2: text2,
      filepath1: full_path1,
      filepath2: full_path2,
      isInternal: data.file1.is_internal,
      label1: label1,
      label2: label2,
      isSmall: text1.length < bigFileSize && text2.length < bigFileSize,
      isTiedToMonaco: isFromMonaco,
      lang: lang
    }));
    setActiveTab('COMPARER');
  } catch (error) {
    console.error('ERROR compareInternalFileWithOriginal: ', error);
  }

}
export async function compareInternalFileWithOVanilaMonaco(setStatusText, setActiveTab, setCompareData, editorRef, setLabelTextDisplay) {
  try {
    const isFromSarc = false;
    const isFromMonaco = !isFromSarc;
    const path = '';
    const text1 = editorRef.current?.getValue();
    if (!text1 || text1.length === 0) {
      setStatusText('No content in editor!');
      return;
    }
    if (text1.length > MAX_COMPARE_SIZE) {
      const sizeFloat = parseFloat(text1.length);
      setStatusText(`Text size: ${(sizeFloat / 1024.0 / 1024.0).toFixed(2)} MB exceeds the limit: ${MAX_COMPARE_SIZE / 1024 / 1024} MB!`);
      return;
    }
    const content = await invoke('compare_internal_file_with_vanila', { internalPath: path, isFromSarc: isFromSarc });
    if (!content) {
      console.log("No content returned from compare_internal_file_with_vanila");
      return;
    }
    setStatusText(content.status_text);
    if (content.status_text.toLowerCase().startsWith('error:')) {
      console.error(content.status_text);
      return;
    }
    //It worked, asign the data to the compareData
    const data = content.compare_data;
    const text2 = data.file2.text ?? '';
    if (text1 === text2) {
      await discardActiveComparisonDocument();
      setStatusText('Files are identical! Skipping comparison.');
      return;
    }
    const full_path1 = data.file1.path.full_path.replace(/\\/g, '/');
    const full_path2 = data.file2.path.full_path.replace(/\\/g, '/');
    let label1 = data.file1.label;
    label1 = label1.length > 0 ? label1 : full_path1;
    let label2 = data.file2.label;
    label2 = label2.length > 0 ? label2 : full_path2;
    const lang = content.lang ?? 'yaml';
    setCompareData((prevData) => ({
      ...prevData,
      content1: text1,
      content2: text2,
      filepath1: full_path1,
      filepath2: full_path2,
      isInternal: data.file1.is_internal,
      label1: label1,
      label2: label2,
      isSmall: text1.length < bigFileSize && text2.length < bigFileSize,
      isTiedToMonaco: isFromMonaco,
      lang: lang
    }));
    setActiveTab('COMPARER');
    setLabelTextDisplay((prevData) => ({ ...prevData, comparer: content.file_label.replace(/\/\//g, '/') }));
    console.log(content.file_label);
  } catch (error) {
    console.error('ERROR compareInternalFileWithOriginal: \n', error);
  }

}

export async function compareFilesByDecision(setStatusText, setActiveTab, setCompareData, editorRef, isFromDisk, setLabelTextDisplay) {
  try {
    const isFromMonaco = !isFromDisk;
    const editorText = editorRef.current?.getValue() ?? '';
    // const decision = compareData.decision ?? 'FilesFromDisk';
    // const path = compareData.filepath1 ?  '' : intOrRegularPath;
    const content = await invoke('compare_files', {
      // decision: decision,
      // intOrRegularPath: path,
      isFromDisk: isFromDisk
    });
    if (!content) {
      console.log("No content returned from compare_files");
      return;
    }
    setStatusText(content.status_text);
    if (content.status_text.toLowerCase().startsWith('error:')) {
      console.error(content.status_text);
      return;
    }
    const data = content.compare_data;
    const text1 = data.file1.text ?? '';
    const resolvedText1 = text1.length > 0 ? text1 : editorText;
    const text2 = data.file2.text ?? '';
    if (resolvedText1 === text2) {
      await discardActiveComparisonDocument();
      setStatusText('Files are identical! Skipping comparison.');
      return;
    }

    const full_path1 = data.file1.path.full_path.replace(/\\/g, '/');
    const full_path2 = data.file2.path.full_path.replace(/\\/g, '/');
    let label1 = data.file1.label;
    label1 = label1.length > 0 ? label1 : full_path1;
    let label2 = data.file2.label;
    label2 = label2.length > 0 ? label2 : full_path2;
    const lang = content.lang ?? 'yaml';
    setCompareData((prevData) => ({
      ...prevData,
      content2: text2,
      filepath1: full_path1,
      filepath2: full_path2,
      isInternal: data.file1.is_internal,
      label1: label1,
      label2: label2,
      content1: resolvedText1,

      isTiedToMonaco: isFromMonaco,
      lang: lang
    }));
    setLabelTextDisplay((prevData) => ({ ...prevData, comparer: content.file_label.replace(/\/\//g, '/') }));
    console.log(content.file_label);

    //console.log(data.file1.text, data.file2.text);
    if (content.tab === 'ERROR') {
      console.log("Error opening file, no tab set");
    }
    setActiveTab('COMPARER');
  } catch (error) {
    console.error('Failed save as data: ', error);
  }
}

const CompareFiles = () => {
  const {
    compareData,
    settings,
  } = useEditorContext();

  const diffEditorRef = useRef(null);
  const diffChangesRef = useRef([]);
  const currentDiffIndexRef = useRef(-1);
  const [mountedDiffEditor, setMountedDiffEditor] = useState(null);
  const [totalDiffs, setTotalDiffs] = useState(0);

  useEffect(() => {
    if (mountedDiffEditor) {
      const updateDiffCount = () => {
        const changes = mountedDiffEditor.getLineChanges() ?? [];
        diffChangesRef.current = changes;
        currentDiffIndexRef.current = -1;
        setTotalDiffs(changes.length);
      };
      updateDiffCount();
      const subscription = mountedDiffEditor.onDidUpdateDiff(updateDiffCount);

      return () => {
        subscription.dispose();
        diffChangesRef.current = [];
        currentDiffIndexRef.current = -1;
      };
    }
  }, [mountedDiffEditor]);

  const revealDifference = (direction) => {
    const editor = diffEditorRef.current;
    if (!editor) return;
    const changes = editor.getLineChanges() ?? diffChangesRef.current;
    if (changes.length === 0) return;
    diffChangesRef.current = changes;
    const current = currentDiffIndexRef.current;
    const index = direction > 0
      ? (current + 1) % changes.length
      : (current <= 0 ? changes.length - 1 : current - 1);
    currentDiffIndexRef.current = index;
    const change = changes[index];
    const originalLine = change.originalStartLineNumber || change.originalEndLineNumber || 1;
    const modifiedLine = change.modifiedStartLineNumber || change.modifiedEndLineNumber || 1;
    const originalEditor = editor.getOriginalEditor();
    const modifiedEditor = editor.getModifiedEditor();
    originalEditor.revealLineInCenter(originalLine);
    modifiedEditor.revealLineInCenter(modifiedLine);
    originalEditor.setPosition({ lineNumber: originalLine, column: 1 });
    modifiedEditor.setPosition({ lineNumber: modifiedLine, column: 1 });
    modifiedEditor.focus();
  };

  const handleNextDiff = () => {
    revealDifference(1);
  };

  const handlePrevDiff = () => {
    revealDifference(-1);
  };
  const fontSize = 15;
  const padding = 4;
  const margin = 6;
  const buttonStyle = {marginRight: margin, padding: '5px' };
  const divButtonStyle = {padding: '3px', background: '#1e1e1e', color: 'white', display: 'flex', alignItems: 'center', fontSize: fontSize, marginLeft: margin };
  return (
    <div >
    {/* <div > */}
      {/* Navigation Buttons */}
      <div  style={divButtonStyle}>
        <button onClick={handlePrevDiff} style={buttonStyle}>Previous Difference</button>
        <button onClick={handleNextDiff} style={buttonStyle}>Next Difference</button>
        <span style={{marginLeft: margin}}>Total: {totalDiffs}</span>
      </div>

      {/* Labels */}
      <div  style={{
        display: 'flex',
        justifyContent: 'space-between',
        padding: padding,
        background: '#252526',
        color: 'white',
        fontWeight: 'bold',
        fontSize: fontSize,
      }}>
        <div style={{marginLeft: margin}}>{(compareData.label1 ?? '').replace(/\/\//g, '/') || 'Modified File'}</div>
        <div style={{marginRight: margin}}>{(compareData.label2 ?? '').replace(/\/\//g, '/') || 'Original File'}</div>
      </div>

      <div style={{ height: 'calc(var(--app-height, 100vh) - 177px)', width: '100%', flexDirection: 'column' }}>
      {/* DiffEditor */}
        <DiffEditor
        // style={{    overflow: 'hidden'  }}
          original={compareData.content1 || ''}
          modified={compareData.content2 || ''}
          language={compareData.lang || 'plaintext'}
          theme={settings.theme || 'vs-dark'}
          options={{
            readOnly: true,
            renderSideBySide: true,
          }}
          onMount={(editor, monacoInstance) => {
            diffEditorRef.current = editor; // store the real DiffEditor instance
            setMountedDiffEditor(editor);
          }}
          onUnmount={() => {
            diffEditorRef.current = null;
            setMountedDiffEditor(null);
          }}
        />
      </div>
    </div>
  );
};


// export default CompareFiles;




const Comparer = ({ setStatusText, activeTab }) => {
  const {
    compareData,
    setCompareData,
    editorRef,
    setActiveTab,
    setLabelTextDisplay,
    settings
  } = useEditorContext();

  const focusOnLine = (lineNumber) => {
    try {
      if (editorRef.current) {
        editorRef.current.revealLineInCenter(lineNumber);
        editorRef.current.setPosition({ lineNumber, column: 1 });
        editorRef.current.focus();
        setActiveTab('YAML');
      }
    } catch (error) {
      console.error(error);
      setStatusText('Error: ' + error.message);
    }
  };

  useEffect(() => {
    setCompareData((prevData) => ({
      ...prevData,
      isSmall: compareData.content1?.length < bigFileSize && compareData.content2?.length < bigFileSize,
    }));
  }, [compareData.content1, compareData.content2]);

  useEffect(() => {
    console.log(compareData.label1);
    const isBig = compareData.content1?.length > bigFileSize || compareData.content2?.length > bigFileSize;
    // setLabelTextDisplay((prevData) => ({ ...prevData, comparer: isBig ? 'Big file' : 'Small file' }));
    if (compareData.isTiedToMonaco) {
      const lineElements = document.querySelectorAll('.react-diff-127lblb-gutter');

      const handleClick = (event) => {
        const numericElement = Array.from(event.currentTarget.children).find((child) =>
          !isNaN(parseInt(child.textContent, 10))
        );

        if (numericElement) {
          const lineNumber = parseInt(numericElement.textContent, 10);
          if (!isNaN(lineNumber)) {
            focusOnLine(lineNumber);
            console.log('Clicked on line number:', lineNumber);
          }
        }
      };

      lineElements.forEach((element) => {
        element.addEventListener('click', handleClick);
      });

      return () => {
        lineElements.forEach((element) => {
          element.removeEventListener('click', handleClick);
        });
      };
    }
  }, [compareData.isTiedToMonaco, compareData.label1]);

  const doIcompare = true;

  return (
    <div
      // className="diff-viewer-div"
      style={{
        display: activeTab === 'COMPARER' ? 'block' : 'none',
        height: activeTab === 'COMPARER' ? 'auto' : '0%',
        width: activeTab === 'COMPARER' ? '100%' : '0%',
        marginLeft: activeTab === 'COMPARER' ? '0' : '-50px',
        paddingBottom: '50px',
      }}
    >
      <div  style={{ display: activeTab === 'COMPARER' ? "block" : "none" }}>
      {/* <DiffEditor
        original={compareData.content1 ?? ''}
        modified={compareData.content2 ?? ''}
        language={compareData.lang ?? 'yaml'} // Adjust language as needed
        theme={settings.theme} // Choose between "vs-light", "vs-dark", etc.
        options={{
          readOnly: true, // Makes the editor read-only
          renderSideBySide: true, // Side-by-side diff view
        }}
      /> */}
      <CompareFiles />
      </div>

      {/* {doIcompare ?? ( */}
      {/* <ReactDiffViewer
          className="diff"
          oldValue={compareData.content1 || ''}
          newValue={compareData.content2 || ''}
          showDiffOnly={compareData.isSmall}
          splitView={true}
          useDarkTheme={true}
          leftTitle={compareData.label1}
          rightTitle={compareData.label2}
        /> */}
      {/* // ) } */}
    </div>
  );
};

export default Comparer;
