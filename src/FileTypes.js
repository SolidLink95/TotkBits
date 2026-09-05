const UNSAVEABLE_FILE_TYPES = new Set(['BPHCL', 'HKCL', 'HKRG', 'BPHHB', 'BPHYSSB', 'GLB', 'LM3', 'MII', 'OTHER']);

export const isFileTypeSaveable = (fileType) => !UNSAVEABLE_FILE_TYPES.has(fileType || 'NONE');
