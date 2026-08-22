const UNSAVEABLE_FILE_TYPES = new Set(['BPHCL', 'HKCL', 'BPHHB', 'GLB', 'LM3', 'MII', 'OTHER']);

export const isFileTypeSaveable = (fileType) => !UNSAVEABLE_FILE_TYPES.has(fileType || 'NONE');
