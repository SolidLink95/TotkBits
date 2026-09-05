import { invoke } from '@tauri-apps/api/core';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import './PhysicsMerge.css';
import { activateDocument, getDocumentsSnapshot } from './DocumentState';

const locationLabel = { disk: 'Disk', archive: 'Archive', 'nested-archive': 'Nested archive' };
const formatLabel = { hkcl: 'HKCL', bphcl: 'BPHCL', bphhb: 'BPHHB', bphyssb: 'BPHYSSB' };
const isSidecar = (format) => format === 'bphhb' || format === 'bphyssb';

function PhysicsMerge({ activeTab, setActiveTab, returnTab, setStatusText, setpaths, documentSnapshots }) {
    const [documents, setDocuments] = useState({ hkcl: [], bphcl: [], bphhb: [], sidecars: { bphhb: [], bphyssb: [] } });
    const [targetFormat, setTargetFormat] = useState('bphcl');
    const [sourceFormat, setSourceFormat] = useState('bphcl');
    const [targetId, setTargetId] = useState('');
    const [sourceId, setSourceId] = useState('');
    const [helperId, setHelperId] = useState('');
    const [templateIndex, setTemplateIndex] = useState(0);
    const [nodes, setNodes] = useState([]);
    const [selectedNodeIds, setSelectedNodeIds] = useState(new Set());
    const [validation, setValidation] = useState(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState('');
    const [merging, setMerging] = useState(false);
    const refreshSequence = useRef(0);

    const refreshDocuments = useCallback(async () => {
        const sequence = ++refreshSequence.current;
        setLoading(true);
        setError('');
        try {
            const [hkcl, bphcl, bphhb, sidecarList] = await Promise.all([
                invoke('list_open_hkcl_documents'),
                invoke('list_open_bphcl_documents'),
                invoke('list_open_bphhb_documents'),
                invoke('list_open_sidecar_documents'),
            ]);
            const sidecars = {
                bphhb: sidecarList.filter((document) => document.format === 'bphhb'),
                bphyssb: sidecarList.filter((document) => document.format === 'bphyssb'),
            };
            if (sequence === refreshSequence.current) setDocuments({ hkcl, bphcl, bphhb, sidecars });
        } catch (reason) {
            if (sequence !== refreshSequence.current) return;
            const message = String(reason);
            setError(message);
            setStatusText(`ERROR: ${message}`);
        } finally {
            if (sequence === refreshSequence.current) setLoading(false);
        }
    }, [setStatusText]);

    useEffect(() => {
        if (activeTab !== 'PHYSICS_MERGE') return;
        // IDs are allocated per open operation. Do not let the node effect use
        // IDs retained from a previous visit while the fresh list is loading.
        setDocuments({ hkcl: [], bphcl: [], bphhb: [], sidecars: { bphhb: [], bphyssb: [] } });
        setTargetId('');
        setSourceId('');
        setHelperId('');
        setNodes([]);
        setSelectedNodeIds(new Set());
        setValidation(null);
        setError('');
        refreshDocuments();
    }, [activeTab, refreshDocuments]);

    const documentsFor = useCallback((format) => (isSidecar(format) ? documents.sidecars[format] : documents[format]) || [], [documents]);

    useEffect(() => {
        const targets = documentsFor(targetFormat);
        const sources = documentsFor(sourceFormat);
        const active = getDocumentsSnapshot().activeDocumentId;
        setTargetId((current) => targets.some((item) => item.documentId === current)
            ? current : targets[0]?.documentId || '');
        setSourceId((current) => sources.some((item) => item.documentId === current && item.documentId !== targetId)
            ? current : sources.find((item) => item.documentId === active && item.documentId !== targetId)?.documentId
                || sources.find((item) => item.documentId !== targetId)?.documentId || '');
    }, [documents, documentsFor, sourceFormat, targetFormat, targetId]);

    useEffect(() => {
        setHelperId((current) => current && documents.bphhb.some((item) => item.documentId === current)
            ? current : '');
    }, [documents.bphhb]);

    useEffect(() => {
        if (!sourceId || activeTab !== 'PHYSICS_MERGE') {
            setNodes([]);
            return;
        }
        let cancelled = false;
        setNodes([]);
        setLoading(true);
        const listNodes = isSidecar(sourceFormat)
            ? invoke('list_sidecar_driver_groups', { documentId: sourceId }).then((groups) => groups.map((group) => ({
                nodeId: `group:${group.index}`,
                kind: 'group',
                index: group.index,
                name: group.driverBoneNames.length ? group.driverBoneNames.join(' + ') : `Driver group ${group.index}`,
                detail: `${group.drivenBoneNames.length} driven, ${group.curveCount} curves, ${group.outputCount} outputs`,
            })))
            : invoke(sourceFormat === 'hkcl' ? 'list_hkcl_selectable_nodes' : 'list_bphcl_selectable_nodes', { documentId: sourceId });
        listNodes
            .then((result) => { if (!cancelled) { setNodes(result); setError(''); } })
            .catch((reason) => { if (!cancelled) setError(String(reason)); })
            .finally(() => { if (!cancelled) setLoading(false); });
        setSelectedNodeIds(new Set());
        return () => { cancelled = true; };
    }, [activeTab, sourceFormat, sourceId]);

    const crossFormat = sourceFormat !== targetFormat;
    const request = useMemo(() => ({
        targetDocumentId: targetId,
        sourceDocumentId: sourceId,
        targetFormat,
        sourceFormat,
        nodeIds: Array.from(selectedNodeIds),
        templateClothIndex: crossFormat ? Number(templateIndex) : null,
        helperDocumentId: sourceFormat === 'bphcl' && targetFormat === 'hkcl' && helperId ? helperId : null,
    }), [crossFormat, helperId, selectedNodeIds, sourceFormat, sourceId, targetFormat, targetId, templateIndex]);

    useEffect(() => {
        if (!targetId || !sourceId || selectedNodeIds.size === 0 || targetId === sourceId) {
            setValidation(null);
            return;
        }
        if (sourceFormat === 'bphcl' && targetFormat === 'bphcl') {
            setValidation({ valid: true, issues: [], requiresTemplate: false, supportsCollidables: true });
            return;
        }
        if (isSidecar(sourceFormat) || isSidecar(targetFormat)) {
            setValidation(sourceFormat === targetFormat
                ? { valid: true, issues: [], requiresTemplate: false, supportsCollidables: false }
                : { valid: false, issues: ['Driver-group merging is same-format only: convert the donor between BPHHB and BPHYSSB first.'] });
            return;
        }
        let cancelled = false;
        invoke('validate_physics_merge_request', { request })
            .then((result) => { if (!cancelled) setValidation(result); })
            .catch((reason) => { if (!cancelled) setValidation({ valid: false, issues: [String(reason)] }); });
        return () => { cancelled = true; };
    }, [request, selectedNodeIds.size, sourceId, targetId]);

    const groups = useMemo(() => (isSidecar(sourceFormat)
        ? [['group', 'Driver groups']]
        : [['cloth', 'Complete cloths'], ['collidable', 'Standalone collidables']]
    ).map(([kind, label]) => ({
        kind, label, nodes: nodes.filter((node) => node.kind === kind && (!crossFormat || kind !== 'collidable')),
    })), [crossFormat, nodes, sourceFormat]);

    const toggleNode = (nodeId) => setSelectedNodeIds((current) => {
        const next = new Set(current);
        if (next.has(nodeId)) next.delete(nodeId); else next.add(nodeId);
        return next;
    });

    const toggleGroup = (groupNodes) => setSelectedNodeIds((current) => {
        const next = new Set(current);
        const ids = groupNodes.map((node) => node.nodeId);
        const allSelected = ids.length > 0 && ids.every((id) => current.has(id));
        ids.forEach((id) => allSelected ? next.delete(id) : next.add(id));
        return next;
    });

    const selectableNodes = useMemo(() => groups.flatMap((group) => group.nodes), [groups]);
    const allSelected = (groupNodes) => groupNodes.length > 0
        && groupNodes.every((node) => selectedNodeIds.has(node.nodeId));

    const swapDocuments = () => {
        setTargetFormat(sourceFormat);
        setSourceFormat(targetFormat);
        setTargetId(sourceId);
        setSourceId(targetId);
        setHelperId('');
        setTemplateIndex(0);
        setSelectedNodeIds(new Set());
        setValidation(null);
        setError('');
    };

    const mergeSelection = async () => {
        if (!validation?.valid) return;
        setMerging(true);
        setError('');
        try {
            if (isSidecar(sourceFormat) && sourceFormat === targetFormat) {
                let result = null;
                const indices = request.nodeIds.map((id) => Number(id.split(':')[1])).sort((a, b) => a - b);
                for (const groupIndex of indices) {
                    result = await invoke('merge_sidecar_driver_group', { targetDocumentId: targetId, sourceDocumentId: sourceId, groupIndex });
                }
                const mergeStatus = `${formatLabel[targetFormat]} driver-group merge completed: ${indices.length} group(s) imported, ${result.groupCount} groups and ${result.boneCount} bones in target`;
                const refreshedPaths = { ...result.sarcPaths, documentId: targetId };
                const snapshot = documentSnapshots.current.get(targetId);
                if (snapshot) documentSnapshots.current.set(targetId, { ...snapshot, activeTab: 'SARC', paths: refreshedPaths, statusText: mergeStatus });
                if (getDocumentsSnapshot().activeDocumentId !== targetId) activateDocument(targetId);
                setpaths(refreshedPaths);
                setActiveTab('SARC');
                setStatusText(mergeStatus);
            } else if (sourceFormat === 'bphcl' && targetFormat === 'bphcl') {
                const result = await invoke('merge_bphcl_nodes', {
                    targetDocumentId: targetId, sourceDocumentId: sourceId, nodeIds: request.nodeIds,
                });
                const mergeStatus = `Physics merge completed: ${result.importedCount} nodes added (${result.addedClothCount} cloth, ${result.addedCollidableCount} collidables), ${result.skippedCount} selections skipped`;
                const refreshedPaths = { ...result.sarcPaths, documentId: targetId };
                const snapshot = documentSnapshots.current.get(targetId);
                if (snapshot) documentSnapshots.current.set(targetId, { ...snapshot, activeTab: 'SARC', paths: refreshedPaths, statusText: mergeStatus });
                if (getDocumentsSnapshot().activeDocumentId !== targetId) activateDocument(targetId);
                setpaths(refreshedPaths);
                setActiveTab('SARC');
                setStatusText(mergeStatus);
            } else if (sourceFormat === 'hkcl' && targetFormat === 'bphcl') {
                const result = await invoke('merge_hkcl_nodes_into_bphcl', { request });
                const mergeStatus = `HKCL → BPHCL merge completed: ${result.addedClothCount} cloth and ${result.addedCollidableCount} collidables added`;
                const refreshedPaths = { ...result.sarcPaths, documentId: targetId };
                const snapshot = documentSnapshots.current.get(targetId);
                if (snapshot) documentSnapshots.current.set(targetId, { ...snapshot, activeTab: 'SARC', paths: refreshedPaths, statusText: mergeStatus });
                if (getDocumentsSnapshot().activeDocumentId !== targetId) activateDocument(targetId);
                setpaths(refreshedPaths);
                setActiveTab('SARC');
                setStatusText(mergeStatus);
            } else {
                const result = await invoke('build_physics_merge_graph', { request });
                setStatusText(`Physics merge graph built: ${result.imported.length} imported. Binary document update is not available yet.`);
            }
            setSelectedNodeIds(new Set());
        } catch (reason) {
            const message = String(reason);
            setError(message);
            setStatusText(`ERROR: ${message}`);
        } finally {
            setMerging(false);
        }
    };

    if (activeTab !== 'PHYSICS_MERGE') return null;
    const targetDocuments = documentsFor(targetFormat);
    const sourceDocuments = documentsFor(sourceFormat);
    const target = targetDocuments.find((item) => item.documentId === targetId);

    return <section className="physics-merge-view" aria-labelledby="physics-merge-title">
        {merging && <div className="parsing-overlay" role="status"><div className="parsing-content"><div className="loading-swirl"></div><div>Building physics merge…</div></div></div>}
        <header className="physics-merge-header">
            <div className="physics-merge-title"><h1 id="physics-merge-title">Physics Merge</h1><p>Merge HKCL and BPHCL cloth physics, or BPHHB and BPHYSSB driver groups, between open documents.</p></div>
            <div className="physics-merge-actions"><button onClick={refreshDocuments} disabled={loading}>Refresh</button><button onClick={() => setActiveTab(returnTab)}>Close</button></div>
        </header>

        <div className="physics-merge-documents physics-merge-formats">
            <label>Target format<select value={targetFormat} onChange={(event) => setTargetFormat(event.target.value)}><option value="bphcl">BPHCL</option><option value="hkcl">HKCL</option><option value="bphhb">BPHHB</option><option value="bphyssb">BPHYSSB</option></select></label>
            <label>Target document<select value={targetId} onChange={(event) => setTargetId(event.target.value)}>{targetDocuments.map((document) => <option key={document.documentId} value={document.documentId} disabled={document.documentId === sourceId}>{document.label} — {locationLabel[document.location]}</option>)}</select></label>
            <label>Source format<select value={sourceFormat} onChange={(event) => setSourceFormat(event.target.value)}><option value="bphcl">BPHCL</option><option value="hkcl">HKCL</option><option value="bphhb">BPHHB</option><option value="bphyssb">BPHYSSB</option></select></label>
            <label>Source document<select value={sourceId} onChange={(event) => setSourceId(event.target.value)}>{sourceDocuments.filter((document) => document.documentId !== targetId).map((document) => <option key={document.documentId} value={document.documentId}>{document.label} — {locationLabel[document.location]}</option>)}</select></label>
            <button className="physics-merge-swap" type="button" onClick={swapDocuments} disabled={!targetId || !sourceId}>Swap source / target</button>
        </div>

        {crossFormat && !isSidecar(sourceFormat) && !isSidecar(targetFormat) && <div className="physics-merge-documents physics-merge-options">
            <label>Target template cloth<select value={templateIndex} onChange={(event) => setTemplateIndex(event.target.value)}>{Array.from({ length: target?.clothCount || 0 }, (_, index) => <option key={index} value={index}>Cloth {index}</option>)}</select></label>
            {sourceFormat === 'bphcl' && targetFormat === 'hkcl' && <label>BPHHB helper (optional)<select value={helperId} onChange={(event) => setHelperId(event.target.value)}><option value="">Direct bone mapping</option>{documents.bphhb.map((document) => <option key={document.documentId} value={document.documentId}>{document.label} — {document.boneCount} bones</option>)}</select></label>}
        </div>}

        {error && <div className="physics-merge-error" role="alert">{error}</div>}
        {validation && !validation.valid && <div className="physics-merge-error" role="alert">{validation.issues.map((issue) => <div key={issue}>{issue}</div>)}</div>}
        {targetFormat === 'hkcl' && <p className="physics-merge-notice">HKCL target merge results are validated graph previews until HKCL binary rebuilding is implemented.</p>}
        <div className="physics-merge-selection-actions">
            <button type="button" onClick={() => toggleGroup(selectableNodes)} disabled={loading || selectableNodes.length === 0}>{allSelected(selectableNodes) ? 'Deselect all nodes' : 'Select all nodes'}</button>
            {groups.map((group) => <button type="button" key={group.kind} onClick={() => toggleGroup(group.nodes)} disabled={loading || group.nodes.length === 0}>{allSelected(group.nodes) ? `Deselect all ${group.label.toLowerCase()}` : `Select all ${group.label.toLowerCase()}`}</button>)}
        </div>
        {loading && <div className="physics-merge-empty">Loading {formatLabel[sourceFormat]} nodes…</div>}
        {!loading && groups.map((group) => <div className="physics-merge-group" key={group.kind}><h2>{group.label} <span>{group.nodes.length}</span></h2>{group.nodes.length === 0 ? <p className="physics-merge-empty">No selectable {group.label.toLowerCase()}.</p> : <div className="physics-merge-node-list">{group.nodes.map((node) => <label className="physics-merge-node" key={node.nodeId}><input type="checkbox" checked={selectedNodeIds.has(node.nodeId)} onChange={() => toggleNode(node.nodeId)} /><span className="physics-merge-node-name">{node.name || `Unnamed ${node.kind} ${node.index}`}</span><span className="physics-merge-node-meta">{node.kind === 'group' ? node.detail : sourceFormat === 'bphcl' ? `ITEM ${node.itemIndex}` : `DATA 0x${node.dataOffset.toString(16)}`}</span></label>)}</div>}</div>)}

        <footer className="physics-merge-footer"><span>{selectedNodeIds.size} selected</span><button onClick={mergeSelection} disabled={merging || loading || !validation?.valid}>{merging ? 'Merging…' : targetFormat === 'hkcl' ? 'Build merge graph' : 'Merge selected'}</button></footer>
    </section>;
}

export default PhysicsMerge;
