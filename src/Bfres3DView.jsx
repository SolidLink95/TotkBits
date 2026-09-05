import Editor from '@monaco-editor/react';
import { Canvas, useFrame, useThree } from '@react-three/fiber';
import { Grid, OrbitControls, PerspectiveCamera } from '@react-three/drei';
import { open, save } from '@tauri-apps/plugin-dialog';
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import * as THREE from 'three';
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';
import { getDocumentsSnapshot, invoke, subscribeDocuments } from './DocumentState';
import Tomodachi, { findTomodachiTexture } from './Tomodachi';
import './Bfres3DView.css';

const celGradient = new THREE.DataTexture(
    new Uint8Array([45, 115, 190, 255]),
    4,
    1,
    THREE.RedFormat,
);
celGradient.minFilter = THREE.NearestFilter;
celGradient.magFilter = THREE.NearestFilter;
celGradient.generateMipmaps = false;
celGradient.needsUpdate = true;

// Keep inspected models while their document is open so switching tabs does
// not invoke Rust or transfer the full mesh payload again.
const modelInspectionCache = new Map();
const g1aInspectionCache = new Map();
const g1aInspectionFailures = new Map();

const importDuration = (milliseconds) => {
    const seconds = Math.floor(milliseconds / 1000);
    return `${String(Math.floor(seconds / 60)).padStart(2, '0')}m ${String(seconds % 60).padStart(2, '0')}s`;
};
const playbackTime = (seconds) => `${Math.floor(seconds / 60)}:${(seconds % 60).toFixed(2).padStart(5, '0')}`;
const cacheModelInspection = (path, value) => {
    modelInspectionCache.set(path, value);
};

const attributeRows = (attribute, width) => {
    if (!attribute) return [];
    return Array.from({ length: attribute.count }, (_, index) =>
        Array.from({ length: width }, (__, component) => attribute.array[index * attribute.itemSize + component] ?? 0));
};

const textureDataUrl = async (texture) => {
    const image = texture?.source?.data || texture?.image;
    if (!image) return null;
    const width = image.width || image.videoWidth;
    const height = image.height || image.videoHeight;
    if (!width || !height) return null;
    const canvas = document.createElement('canvas');
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext('2d');
    context.drawImage(image, 0, 0, width, height);
    return {
        dataUrl: canvas.toDataURL('image/png'),
        width,
        height,
        wrapS: texture.wrapS,
        wrapT: texture.wrapT,
        repeat: texture.repeat?.toArray() || [1, 1],
        offset: texture.offset?.toArray() || [0, 0],
        center: texture.center?.toArray() || [0, 0],
        rotation: texture.rotation || 0,
        magFilter: texture.magFilter,
        minFilter: texture.minFilter,
    };
};

const decodeBase64Bytes = (encoded) => Uint8Array.from(atob(encoded), (character) => character.charCodeAt(0));

const inspectGlb = async (title, documentId) => {
    const encoded = await invoke('read_glb_preview', { documentId });
    return inspectGlbBytes(title, decodeBase64Bytes(encoded));
};

const inspectGlbPath = async (path) => {
    const encoded = await invoke('read_file_base64', { path });
    const name = path.replace(/\\/g, '/').split('/').pop();
    return inspectGlbBytes(name, decodeBase64Bytes(encoded));
};

const inspectGlbBytes = async (title, bytes) => {
    const gltf = await new Promise((resolve, reject) => {
        new GLTFLoader().parse(bytes.buffer, '', resolve, reject);
    });
    gltf.scene.updateMatrixWorld(true);

    const textureKinds = [
        ['map', 'Base color', '_a0'],
        ['normalMap', 'Normal', '_n0'],
        ['roughnessMap', 'Roughness', '_r0'],
        ['metalnessMap', 'Metalness', '_m0'],
        ['emissiveMap', 'Emission', '_e0'],
        ['alphaMap', 'Mask', '_a1'],
        ['aoMap', 'AmbientOcclusion', '_o0'],
    ];
    const materialMap = new Map();
    const textureMap = new Map();
    const materials = [];
    const registerMaterial = (material) => {
        if (materialMap.has(material.uuid)) return materialMap.get(material.uuid);
        const textureSlots = [];
        textureKinds.forEach(([property, textureType, sampler], slotIndex) => {
            const texture = material[property];
            if (!texture) return;
            const name = texture.name || texture.source?.name || `${material.name || 'Material'} ${textureType}`;
            textureMap.set(texture.uuid, { texture, name });
            textureSlots.push({ index: slotIndex, name, texture_type: textureType, sampler, uv_layer: texture.channel || 0 });
        });
        const index = materials.length;
        materials.push({
            name: material.name || `Material ${index}`,
            offset: index,
            texture_slots: textureSlots,
            color: material.color?.toArray() || [1, 1, 1],
            emissive: material.emissive?.toArray() || [0, 0, 0],
            opacity: material.opacity ?? 1,
            transparent: Boolean(material.transparent),
            roughness: material.roughness ?? null,
            metalness: material.metalness ?? null,
        });
        materialMap.set(material.uuid, index);
        return index;
    };

    const meshes = [];
    gltf.scene.traverse((object) => {
        if (!object.isMesh || !object.geometry?.attributes?.position) return;
        const sourceMaterials = Array.isArray(object.material) ? object.material : [object.material];
        sourceMaterials.filter(Boolean).forEach(registerMaterial);
        const geometry = object.geometry;
        const positionAttribute = geometry.attributes.position;
        const normalAttribute = geometry.attributes.normal;
        const colorAttribute = geometry.attributes.color;
        const uvMaps = Object.entries(geometry.attributes)
            .filter(([name]) => /^uv\d*$/.test(name))
            .sort(([left], [right]) => left.localeCompare(right, undefined, { numeric: true }))
            .map(([, attribute]) => attributeRows(attribute, 2));
        const positions = attributeRows(positionAttribute, 3);
        const normals = attributeRows(normalAttribute, 3);
        const normalMatrix = new THREE.Matrix3().getNormalMatrix(object.matrixWorld);
        positions.forEach((position) => new THREE.Vector3().fromArray(position).applyMatrix4(object.matrixWorld).toArray(position));
        normals.forEach((normal) => new THREE.Vector3().fromArray(normal).applyMatrix3(normalMatrix).normalize().toArray(normal));
        const allIndices = geometry.index
            ? Array.from(geometry.index.array)
            : Array.from({ length: positionAttribute.count }, (_, index) => index);
        const groups = geometry.groups.length
            ? geometry.groups
            : [{ start: 0, count: allIndices.length, materialIndex: 0 }];
        groups.forEach((group, groupIndex) => {
            const material = sourceMaterials[group.materialIndex] || sourceMaterials[0];
            const fallbackColor = material?.color?.toArray() || [0.68, 0.72, 0.76];
            const colors = colorAttribute
                ? attributeRows(colorAttribute, Math.min(4, colorAttribute.itemSize))
                : positions.map(() => [...fallbackColor, material?.opacity ?? 1]);
            meshes.push({
                name: groups.length > 1 ? `${object.name || 'Mesh'} [${groupIndex}]` : object.name || `Mesh ${meshes.length}`,
                positions: positions.map((value) => [...value]),
                normals: normals.map((value) => [...value]),
                indices: allIndices.slice(group.start, group.start + group.count),
                uv0: uvMaps[0] || [],
                uv_maps: uvMaps,
                colors,
                material_index: material ? registerMaterial(material) : 0,
                // GLB materials commonly carry their visible colour in the
                // baseColorFactor. Keep that as a material property instead of
                // manufacturing a vertex-colour attribute: doing the latter
                // made the renderer lose opaque surfaces that use color factors.
                material_color: fallbackColor,
                material_opacity: material?.opacity ?? 1,
                simple_glb_material: true,
                use_vertex_colors: false,
                bone_index: 0,
                vertex_skin_count: 0,
                bone_indices: positions.map(() => []),
                bone_weights: positions.map(() => []),
                source_node: object.parent?.name || gltf.scene.name || 'Scene',
            });
        });
    });

    const resolvedTextures = [];
    for (const [uuid, entry] of textureMap) {
        const rendered = await textureDataUrl(entry.texture);
        if (!rendered) continue;
        resolvedTextures.push({
            name: entry.name,
            aliases: [],
            path: `glb://${uuid}`,
            source: 'embedded',
            renderable: true,
            colorSpace: 'srgb',
            ...rendered,
        });
    }
    const modelName = gltf.scene.name || title?.replace(/\.glb$/i, '') || 'GLB Model';
    const animations = gltf.animations.map((animation, index) => ({
        name: animation.name || `Animation ${index}`,
        duration: animation.duration,
        tracks: animation.tracks.length,
    }));
    return {
        format: 'GLB',
        name: modelName,
        sections: [{ signature: [70, 77, 68, 76], name: modelName, offset: 0 }],
        materials,
        resolvedTextures,
        animations,
        render: { meshes, bones: [], scale_mode: 'none' },
    };
};

// TextureLoader otherwise decodes identical data URLs every time a cached
// model becomes active. These textures intentionally live as long as the model
// inspection cache and are released when the AOC configuration invalidates it.
const resolvedTextureCache = new Map();
const clearModelCaches = () => {
    modelInspectionCache.clear();
    g1aInspectionCache.clear();
    g1aInspectionFailures.clear();
    resolvedTextureCache.forEach((texture) => texture.dispose());
    resolvedTextureCache.clear();
};

const mergeG1mModels = (models) => {
    const merged = {
        format: 'G1M',
        name: `${models.length} selected AOC models`,
        sections: [],
        materials: [],
        resolvedTextures: [],
        textureStats: { total: 0, skipped: 0 },
        render: { meshes: [], bones: [], scale_mode: models[0]?.value?.render?.scale_mode || 'none' },
    };
    models.forEach(({ path, value }) => {
        const modelId = path.replace(/\\/g, '/').split('/').pop()?.replace(/\.g1m$/i, '') || 'model';
        const materialOffset = merged.materials.length;
        const boneOffset = merged.render.bones.length;
        merged.sections.push(...(value.sections || []).map((section) => ({
            ...section,
            name: section.name ? `${modelId}: ${section.name}` : modelId,
        })));
        merged.materials.push(...(value.materials || []).map((material) => ({
            ...material,
            name: `${modelId}: ${material.name || 'Material'}`,
            texture_slots: (material.texture_slots || []).map((slot) => ({
                ...slot,
                name: `${modelId}:${slot.name}`,
            })),
        })));
        merged.resolvedTextures.push(...(value.resolvedTextures || []).map((texture) => ({
            ...texture,
            name: `${modelId}:${texture.name}`,
            aliases: (texture.aliases || []).map((alias) => `${modelId}:${alias}`),
        })));
        merged.render.bones.push(...(value.render?.bones || []).map((bone) => ({
            ...bone,
            name: `${modelId}: ${bone.name || 'Bone'}`,
            parent_index: bone.parent_index >= 0 ? bone.parent_index + boneOffset : -1,
        })));
        merged.render.meshes.push(...(value.render?.meshes || []).map((mesh) => ({
            ...mesh,
            name: `${modelId}: ${mesh.name || 'Mesh'}`,
            material_index: mesh.material_index + materialOffset,
            bone_index: mesh.bone_index >= 0 ? mesh.bone_index + boneOffset : mesh.bone_index,
            bone_indices: (mesh.bone_indices || []).map((indices) =>
                indices.map((index) => index + boneOffset)),
            skin_bones: (mesh.skin_bones || []).map((index) => index + boneOffset),
        })));
        merged.textureStats.total += value.textureStats?.total || 0;
        merged.textureStats.skipped += value.textureStats?.skipped || 0;
    });
    return merged;
};

const sectionYaml = (section) => [
    `type: ${section.signature.join ? String.fromCharCode(...section.signature) : section.signature}`,
    `name: ${section.name ?? 'null'}`,
    `offset: 0x${Number(section.offset).toString(16).toUpperCase()}`,
    'parameters: {}',
].join('\n');

const modelTextureStatus = (value) => {
    const requested = new Set((value.materials || []).flatMap((material) => material.texture_slots.map((slot) => slot.name))).size;
    const loaded = value.resolvedTextures?.length || 0;
    if (value.format === 'G1M' && value.textureStats) {
        const unresolved = Math.max(0, value.textureStats.total - value.textureStats.skipped - loaded);
        return `Loaded ${loaded} rendering textures; skipped ${value.textureStats.skipped} of ${value.textureStats.total} references${unresolved ? `; ${unresolved} unresolved` : ''}`;
    }
    return `Loaded ${loaded} of ${requested} referenced textures`;
};

const hasCompleteTextureResolution = (value) => {
    const loaded = value?.resolvedTextures?.length || 0;
    if (value?.format === 'G1M') return loaded > 0;
    const requested = new Set((value?.materials || [])
        .flatMap((material) => (material.texture_slots || []).map((slot) => slot.name))).size;
    return requested === 0 || loaded >= requested;
};

const bindG1aToModel = (animation, model) => {
    const mapping = model?.global_to_local_bones || [];
    const bones = model?.render?.bones || [];
    const tracks = [];
    const unmappedBoneIds = [];
    (animation?.bones || []).forEach((track) => {
        const boneIndex = mapping[track.boneId];
        if (!Number.isInteger(boneIndex) || boneIndex === 0xffff || !bones[boneIndex]) {
            unmappedBoneIds.push(track.boneId);
            return;
        }
        tracks.push({
            globalBoneId: track.boneId,
            boneIndex,
            boneName: bones[boneIndex].name,
            scale: track.scale,
            rotation: track.rotation,
            translation: track.translation,
        });
    });
    return { duration: animation?.header?.duration || 0, tracks, unmappedBoneIds };
};

function boneWorldMatrices(bones, scaleMode = 'none') {
    const matrices = bones.map(() => new THREE.Matrix4());
    bones.forEach((bone, index) => {
        const rotation = bone.rotation_mode === 'euler_xyz'
            ? new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 0, 1), bone.rotation[2])
                .multiply(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), bone.rotation[1]))
                .multiply(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(1, 0, 0), bone.rotation[0]))
            : new THREE.Quaternion(...bone.rotation).normalize();
        const local = new THREE.Matrix4().compose(
            new THREE.Vector3(...bone.translation),
            rotation,
            new THREE.Vector3(...bone.scale),
        );
        if (bone.parent_index >= 0 && matrices[bone.parent_index]) {
            const parent = matrices[bone.parent_index].clone();
            if (scaleMode === 'maya') {
                const scale = bones[bone.parent_index].scale;
                parent.multiply(new THREE.Matrix4().makeScale(
                    scale[0] ? 1 / scale[0] : 1,
                    scale[1] ? 1 / scale[1] : 1,
                    scale[2] ? 1 / scale[2] : 1,
                ));
            }
            matrices[index] = parent.multiply(local);
        } else matrices[index] = local;
    });
    return matrices;
}

const makeThreeAnimationClip = (animation, armatureBones, name = 'G1A') => {
    if (!animation) return null;
    const keyframeTracks = [];
    const addTrack = (track, property, values, TrackType) => {
        if (!values?.length) return;
        keyframeTracks.push(new TrackType(
            `${armatureBones[track.boneIndex].name}.${property}`,
            values.map((key) => key.time),
            values.flatMap((key) => key.value),
        ));
    };
    animation.tracks.forEach((track) => {
        if (!armatureBones[track.boneIndex]) return;
        addTrack(track, 'scale', track.scale, THREE.VectorKeyframeTrack);
        // The Rust parser preserves Project-G1M's transposed quaternion output.
        // Three's armature uses the raw G1M bind convention, so conjugate the
        // animation back while constructing its native track.
        addTrack(track, 'quaternion', track.rotation?.map((key) => ({
            ...key,
            value: [-key.value[0], -key.value[1], -key.value[2], key.value[3]],
        })), THREE.QuaternionKeyframeTrack);
        addTrack(track, 'position', track.translation, THREE.VectorKeyframeTrack);
    });
    return new THREE.AnimationClip(name, animation.duration, keyframeTracks);
};

function useAnimationPose(bones, scaleMode, animation, playing, seek, onTimeUpdate) {
    const restWorlds = useMemo(() => boneWorldMatrices(bones, scaleMode), [bones, scaleMode]);
    const worldsRef = useRef(restWorlds.map((matrix) => matrix.clone()));
    const armature = useMemo(() => {
        const root = new THREE.Group();
        const nodes = bones.map((bone, index) => {
            const node = new THREE.Bone();
            // Keep the bones_botw.json name visible while the index suffix makes
            // Three.js PropertyBinding unambiguous if a catalog has duplicates.
            node.name = `${THREE.PropertyBinding.sanitizeNodeName(bone.name || `bone_${index}`)}__${index}`;
            node.position.fromArray(bone.translation);
            node.quaternion.fromArray(bone.rotation).normalize();
            node.scale.fromArray(bone.scale);
            return node;
        });
        bones.forEach((bone, index) => {
            if (bone.parent_index >= 0 && nodes[bone.parent_index]) nodes[bone.parent_index].add(nodes[index]);
            else root.add(nodes[index]);
        });
        root.updateMatrixWorld(true);
        return { root, nodes };
    }, [bones]);
    const clip = useMemo(() => makeThreeAnimationClip(animation, armature.nodes), [animation, armature]);
    const mixer = useMemo(() => new THREE.AnimationMixer(armature.root), [armature]);
    const reportedAt = useRef(0);
    useEffect(() => {
        worldsRef.current = restWorlds.map((matrix) => matrix.clone());
        mixer.stopAllAction();
        bones.forEach((bone, index) => {
            armature.nodes[index].position.fromArray(bone.translation);
            armature.nodes[index].quaternion.fromArray(bone.rotation).normalize();
            armature.nodes[index].scale.fromArray(bone.scale);
        });
        if (clip) mixer.clipAction(clip).reset().setLoop(THREE.LoopRepeat, Infinity).play();
        return () => {
            mixer.stopAllAction();
            if (clip) mixer.uncacheClip(clip);
        };
    }, [animation, armature, bones, clip, mixer, restWorlds]);
    useEffect(() => {
        if (!clip || seek?.revision === undefined) return;
        mixer.setTime(Math.max(0, Math.min(seek.time, animation.duration)));
        armature.root.updateMatrixWorld(true);
        armature.nodes.forEach((bone, index) => worldsRef.current[index].copy(bone.matrixWorld));
        onTimeUpdate?.(mixer.time);
    }, [seek?.revision]);
    useFrame((_, delta) => {
        if (!clip || !playing) return;
        mixer.update(delta);
        armature.root.updateMatrixWorld(true);
        armature.nodes.forEach((bone, index) => worldsRef.current[index].copy(bone.matrixWorld));
        if (mixer.time - reportedAt.current >= 0.05 || mixer.time < reportedAt.current) {
            reportedAt.current = mixer.time;
            onTimeUpdate?.(mixer.time % animation.duration);
        }
    });
    return { restWorlds, worldsRef };
}

function useResolvedTextures(entries, cacheTextures = true) {
    const [textures, setTextures] = useState(() => ({}));
    useEffect(() => {
        const loader = new THREE.TextureLoader();
        const loaded = {};
        const owned = [];
        let cancelled = false;
        const publish = () => {
            if (!cancelled) setTextures({ ...loaded });
        };
        for (const entry of entries || []) {
            const urls = entry.dataUrls?.length ? entry.dataUrls : [entry.dataUrl];
            const loadLayer = (url, suffix = '') => {
                const cacheKey = `${entry.path || entry.name}:${suffix}:${url}`;
                const cached = cacheTextures ? resolvedTextureCache.get(cacheKey) : null;
                const textureName = suffix ? `${entry.name}::last` : entry.name;
                const aliases = (entry.aliases || []).map((alias) => suffix ? `${alias}::last` : alias);
                const assign = (texture) => {
                    loaded[textureName] = texture;
                    aliases.forEach((alias) => { loaded[alias] = texture; });
                };
                if (cached) {
                    assign(cached);
                    return;
                }
                loader.load(url, (texture) => {
                    if (cancelled) {
                        texture.dispose();
                        return;
                    }
                    texture.name = `${entry.name}${suffix}`;
                    texture.flipY = false;
                    texture.colorSpace = entry.colorSpace === 'srgb'
                        ? THREE.SRGBColorSpace
                        : THREE.NoColorSpace;
                    texture.wrapS = entry.wrapS ?? THREE.RepeatWrapping;
                    texture.wrapT = entry.wrapT ?? THREE.RepeatWrapping;
                    if (entry.repeat) texture.repeat.fromArray(entry.repeat);
                    if (entry.offset) texture.offset.fromArray(entry.offset);
                    if (entry.center) texture.center.fromArray(entry.center);
                    texture.rotation = entry.rotation || 0;
                    texture.magFilter = entry.magFilter ?? texture.magFilter;
                    texture.minFilter = entry.minFilter ?? texture.minFilter;
                    texture.needsUpdate = true;
                    texture.userData.renderable = entry.renderable !== false;
                    assign(texture);
                    if (cacheTextures) resolvedTextureCache.set(cacheKey, texture);
                    else owned.push(texture);
                    publish();
                }, undefined, () => {});
            };
            loadLayer(urls[0]);
            if (urls.length > 1) loadLayer(urls.at(-1), ' [last layer]');
        }
        publish();
        return () => {
            cancelled = true;
            owned.forEach((texture) => texture.dispose());
        };
    }, [entries, cacheTextures]);
    return textures;
}

function materialTextures(material, textures, allowTomodachiFallback = true) {
    if (!material) return {};
    const slotFor = (type) => material.texture_slots.find((value) => value.texture_type === type);
    const find = (type, lastLayer = false) => {
        const slot = slotFor(type);
        const direct = slot ? textures[lastLayer ? `${slot.name}::last` : slot.name] || textures[slot.name] || null : null;
        return direct || (allowTomodachiFallback && !lastLayer ? findTomodachiTexture(textures, type) : null);
    };
    // Never guess the diffuse texture from an unclassified slot. In particular,
    // AO and other packed maps must not become base color merely because they
    // are the first texture referenced by the material.
    const diffuseSlot = material.texture_slots.find((value) => value.sampler?.toLowerCase() === '_a0')
        || slotFor('Base color');
    const base = (diffuseSlot ? textures[diffuseSlot.name] || null : null) || find('Base color');
    // AoC emission arrays store the material image in layer 0. Later layers
    // are auxiliary data and may be black; the legacy importer also selects 0.
    const candidateEmission = find('Emission');
    const emission = candidateEmission?.userData.renderable === false ? null : candidateEmission;
    if (base) base.colorSpace = THREE.SRGBColorSpace;
    if (emission) emission.colorSpace = THREE.SRGBColorSpace;
    if (base) base.channel = 0;
    const normal = find('Normal');
    if (normal) normal.channel = 0;
    return {
        base,
        baseUv: diffuseSlot?.uv_layer ?? 0,
        normal,
        normalUv: material.texture_slots.find((value) => value.texture_type === 'Normal')?.uv_layer ?? 0,
        roughness: find('Roughness'),
        roughnessUv: slotFor('Roughness')?.uv_layer ?? 0,
        metalness: find('Metalness'),
        metalnessUv: slotFor('Metalness')?.uv_layer ?? 0,
        emission,
        emissionUv: slotFor('Emission')?.uv_layer ?? 0,
        mask: find('Mask'),
        maskUv: slotFor('Mask')?.uv_layer ?? 0,
        specular: find('Specular'),
        specularUv: slotFor('Specular')?.uv_layer ?? 0,
        ambientOcclusion: find('AmbientOcclusion'),
        ambientOcclusionUv: slotFor('AmbientOcclusion')?.uv_layer ?? 0,
    };
}

function weightPreviewBoneColor(index) {
    // A golden-angle hue step keeps consecutive bone indices far apart. The
    // alternating lightness adds separation even after weight colors blend.
    const hue = (index * 0.3819660112501051) % 1;
    const saturation = 0.9;
    const lightness = index % 2 === 0 ? 0.48 : 0.64;
    const color = new THREE.Color();
    color.setHSL(hue, saturation, lightness, THREE.SRGBColorSpace);
    return [color.r, color.g, color.b];
}

function buildWeightPreview(render) {
    const boneColors = render.bones.map((_, index) => weightPreviewBoneColor(index));
    return render.meshes.map((mesh) => {
        const colors = new Float32Array(mesh.positions.length * 3);
        for (let vertex = 0; vertex < mesh.positions.length; vertex += 1) {
            const indices = mesh.bone_indices[vertex] || [];
            const weights = mesh.bone_weights[vertex] || [];
            for (let influence = 0; influence < indices.length; influence += 1) {
                const color = boneColors[indices[influence]];
                if (!color) continue;
                const weight = weights[influence] ?? (influence === 0 ? 1 : 0);
                colors[vertex * 3] += color[0] * weight;
                colors[vertex * 3 + 1] += color[1] * weight;
                colors[vertex * 3 + 2] += color[2] * weight;
            }
        }
        return colors;
    });
}

function RenderMesh({ mesh, bones, scaleMode, applyRigidTransform, animation, restWorlds, animationWorlds, culling, viewMode, uvIndex, celShading, glow, weightBone, weightPreviewColors, showNormals, onSelect, textures }) {
    const materialSide = culling ? THREE.FrontSide : THREE.DoubleSide;
    const surfaceColor = !textures.base && !mesh.use_vertex_colors && mesh.material_color
        ? new THREE.Color(mesh.material_color[0], mesh.material_color[1], mesh.material_color[2])
        : new THREE.Color(1, 1, 1);
    const usesMaterialUvs = viewMode === 'default';
    ['base', 'normal', 'roughness', 'metalness', 'emission', 'mask', 'specular', 'ambientOcclusion'].forEach((kind) => {
        if (textures[kind]) textures[kind].channel = usesMaterialUvs ? (textures[`${kind}Uv`] ?? 0) : uvIndex;
    });
    const hasSecondUv = mesh.uv_maps?.[1]?.length === mesh.positions.length;
    if (textures.emission && usesMaterialUvs && hasSecondUv) textures.emission.channel = 1;
    const geometry = useMemo(() => {
        const result = new THREE.BufferGeometry();
        const positions = new Float32Array(mesh.positions.flat());
        const normals = mesh.normals.length === mesh.positions.length ? new Float32Array(mesh.normals.flat()) : null;
        // Smooth-skinned and unskinned vertices are stored in model bind space.
        // BFRES one-bone shapes are stored in bone-local space and need their
        // rest transform restored. G1M rigid vertices are already in model space.
        if (applyRigidTransform && mesh.vertex_skin_count === 1 && bones.length) {
            const worlds = boneWorldMatrices(bones, scaleMode);
            for (let index = 0; index < mesh.positions.length; index += 1) {
                const boneIndex = mesh.bone_indices[index]?.[0] ?? mesh.bone_index;
                const matrix = worlds[boneIndex];
                if (!matrix) continue;
                const position = new THREE.Vector3(positions[index * 3], positions[index * 3 + 1], positions[index * 3 + 2]).applyMatrix4(matrix);
                positions.set(position.toArray(), index * 3);
                if (normals) {
                    const normal = new THREE.Vector3(normals[index * 3], normals[index * 3 + 1], normals[index * 3 + 2])
                        .applyMatrix3(new THREE.Matrix3().getNormalMatrix(matrix)).normalize();
                    normals.set(normal.toArray(), index * 3);
                }
            }
        }
        result.setAttribute('position', new THREE.BufferAttribute(positions, 3));
        if (normals) result.setAttribute('normal', new THREE.BufferAttribute(normals, 3));
        else result.computeVertexNormals();
        const uvMaps = mesh.uv_maps?.length ? mesh.uv_maps : [mesh.uv0];
        const firstUv = uvMaps[0] || mesh.uv0 || [];
        const secondUv = uvMaps[1]?.length === mesh.positions.length ? uvMaps[1] : firstUv;
        if (firstUv.length === mesh.positions.length) {
            result.setAttribute('uv', new THREE.BufferAttribute(new Float32Array(firstUv.flat()), 2));
            result.setAttribute('uv1', new THREE.BufferAttribute(new Float32Array(secondUv.flat()), 2));
            uvMaps.slice(2).forEach((uvMap, index) => {
                if (uvMap.length === mesh.positions.length) {
                    result.setAttribute(`uv${index + 2}`, new THREE.BufferAttribute(new Float32Array(uvMap.flat()), 2));
                }
            });
        }
        const activeUv = uvMaps[uvIndex] || firstUv;
        const colors = new Float32Array(mesh.positions.length * 3);
        mesh.positions.forEach((_, vertex) => {
            let strength = 0;
            if (weightBone >= 0) {
                (mesh.bone_indices[vertex] || []).forEach((bone, influence) => {
                    if (bone === weightBone) strength += mesh.bone_weights[vertex]?.[influence] ?? (influence === 0 ? 1 : 0);
                });
            }
            const normal = mesh.normals[vertex] || [0, 1, 0];
            const uv = activeUv[vertex] || [0, 0];
            let color = new THREE.Color('#aeb8c2');
            if (viewMode === 'weightsPrev' && weightPreviewColors) {
                colors.set(weightPreviewColors.subarray(vertex * 3, vertex * 3 + 3), vertex * 3);
                return;
            }
            if (viewMode === 'selectedBoneWeights') color = new THREE.Color().setHSL((1 - Math.min(strength, 1)) * 0.66, 1, 0.5);
            else if (viewMode === 'default' && weightBone >= 0) {
                color = strength > 0
                    ? new THREE.Color().setHSL((1 - Math.min(strength, 1)) * 0.66, 1, 0.5)
                    : new THREE.Color(0, 0, 0);
            }
            else if ((viewMode === 'vertColor' || (viewMode === 'default' && mesh.use_vertex_colors)) && mesh.colors[vertex]) color = new THREE.Color(mesh.colors[vertex][0], mesh.colors[vertex][1], mesh.colors[vertex][2]);
            else if (viewMode === 'normal' || viewMode === 'normalMap') color = new THREE.Color(normal[0] * 0.5 + 0.5, normal[1] * 0.5 + 0.5, normal[2] * 0.5 + 0.5);
            else if (viewMode === 'uvCoords') color = new THREE.Color(Math.abs(uv[0] % 1), Math.abs(uv[1] % 1), 0.2);
            else if (viewMode === 'uvTestPattern') {
                const checker = (Math.floor(Math.abs(uv[0]) * 16) + Math.floor(Math.abs(uv[1]) * 16)) % 2;
                color = checker ? new THREE.Color(0.92, 0.92, 0.92) : new THREE.Color(0.08, 0.08, 0.08);
            }
            else if (viewMode === 'tangents') color = new THREE.Color(normal[2] * 0.5 + 0.5, normal[0] * 0.5 + 0.5, normal[1] * 0.5 + 0.5);
            else if (viewMode === 'bitangents') color = new THREE.Color(normal[1] * 0.5 + 0.5, normal[2] * 0.5 + 0.5, normal[0] * 0.5 + 0.5);
            else if (viewMode === 'ambientOcclusion') color = new THREE.Color().setScalar(0.2 + Math.max(normal[1], 0) * 0.8);
            else if (viewMode === 'lightMap') color = new THREE.Color().setScalar((Math.abs(uv[0] % 1) + Math.abs(uv[1] % 1)) * 0.5);
            else if (viewMode === 'specularMap') color = new THREE.Color().setScalar(Math.pow(Math.max(normal[2], 0), 8));
            else if (viewMode === 'shadowMap') color = new THREE.Color().setScalar(0.15 + Math.max(normal[1], 0) * 0.45);
            else if (viewMode === 'metalnessMap') color = new THREE.Color().setScalar(0.08);
            else if (viewMode === 'roughnessMap') color = new THREE.Color().setScalar(0.72);
            else if (viewMode === 'subSurfaceScatteringMap') color = new THREE.Color(0.7, 0.12, 0.08);
            else if (viewMode === 'emissionMap') color = new THREE.Color(0.05, 0.02, 0.12);
            colors.set(color.toArray(), vertex * 3);
        });
        result.setAttribute('color', new THREE.BufferAttribute(colors, 3));
        result.setIndex(mesh.indices);
        result.computeBoundingSphere();
        return result;
    }, [mesh, bones, scaleMode, applyRigidTransform, viewMode, uvIndex, weightBone, weightPreviewColors]);
    const restInverse = useMemo(() => restWorlds.map((matrix) => matrix.clone().invert()), [restWorlds]);
    const skinMatrices = useMemo(() => bones.map(() => new THREE.Matrix4()), [bones]);
    const skinPosition = useMemo(() => new THREE.Vector3(), []);
    useFrame(() => {
        if (applyRigidTransform) return;
        const position = geometry.getAttribute('position');
        const posedWorlds = animation ? animationWorlds.current : restWorlds;
        if (!position || !posedWorlds?.length) return;
        for (let bone = 0; bone < skinMatrices.length; bone += 1) {
            skinMatrices[bone].multiplyMatrices(posedWorlds[bone], restInverse[bone]);
        }
        for (let vertex = 0; vertex < mesh.positions.length; vertex += 1) {
            const indices = mesh.bone_indices[vertex] || [];
            const weights = mesh.bone_weights[vertex] || [];
            let x = 0; let y = 0; let z = 0; let total = 0;
            for (let influence = 0; influence < indices.length; influence += 1) {
                const matrix = skinMatrices[indices[influence]];
                if (!matrix) continue;
                const weight = weights[influence] ?? (influence === 0 ? 1 : 0);
                if (weight <= 0) continue;
                skinPosition.fromArray(mesh.positions[vertex]).applyMatrix4(matrix);
                x += skinPosition.x * weight; y += skinPosition.y * weight; z += skinPosition.z * weight;
                total += weight;
            }
            if (total > 0) position.setXYZ(vertex, x / total, y / total, z / total);
            else position.setXYZ(vertex, ...mesh.positions[vertex]);
        }
        position.needsUpdate = true;
        geometry.computeBoundingSphere();
        geometry.computeVertexNormals();
    });
    useEffect(() => () => geometry.dispose(), [geometry]);
    const normalLines = useMemo(() => {
        const lineGeometry = new THREE.BufferGeometry();
        const position = geometry.getAttribute('position');
        const normal = geometry.getAttribute('normal');
        if (!position || !normal) return lineGeometry;
        const length = Math.max(geometry.boundingSphere?.radius || 1, 0.01) * 0.012;
        const step = Math.max(1, Math.ceil(position.count / 4000));
        const points = [];
        for (let index = 0; index < position.count; index += step) {
            const start = new THREE.Vector3().fromBufferAttribute(position, index);
            const end = new THREE.Vector3().fromBufferAttribute(normal, index).normalize().multiplyScalar(length).add(start);
            points.push(...start.toArray(), ...end.toArray());
        }
        lineGeometry.setAttribute('position', new THREE.Float32BufferAttribute(points, 3));
        return lineGeometry;
    }, [geometry]);
    useEffect(() => () => normalLines.dispose(), [normalLines]);
    const selectedEdges = useMemo(() => new THREE.WireframeGeometry(geometry), [geometry]);
    useEffect(() => () => selectedEdges.dispose(), [selectedEdges]);
    return <group>
        <mesh geometry={geometry} visible={!mesh.hidden} onClick={(event) => { event.stopPropagation(); onSelect(mesh); }} castShadow receiveShadow>
            {mesh.simple_glb_material && celShading && ['default', 'lighting', 'wireframe'].includes(viewMode)
                ? <meshToonMaterial key={`simple-glb-cel-${viewMode}-${uvIndex}`} color={surfaceColor} map={textures.base} wireframe={viewMode === 'wireframe'} side={materialSide} transparent={Boolean(textures.base) || (mesh.material_opacity ?? 1) < 1} opacity={mesh.material_opacity ?? 1} alphaTest={textures.base ? 0.02 : 0} />
                : mesh.simple_glb_material && ['default', 'lighting', 'wireframe', 'diffuse'].includes(viewMode)
                ? <meshBasicMaterial key={`simple-glb-${viewMode}-${uvIndex}`} color={surfaceColor} map={textures.base} vertexColors={!textures.base && mesh.use_vertex_colors} wireframe={viewMode === 'wireframe'} side={materialSide} transparent={Boolean(textures.base) || (mesh.material_opacity ?? 1) < 1} opacity={mesh.material_opacity ?? 1} alphaTest={textures.base ? 0.02 : 0} />
                : viewMode === 'normal'
                ? <meshNormalMaterial wireframe={false} side={materialSide} />
                : viewMode === 'blank'
                    ? <meshStandardMaterial color="#aeb8c2" roughness={0.8} metalness={0} side={materialSide} />
                : viewMode === 'normalMap' && textures.normal
                    ? <meshBasicMaterial key={`normal-${uvIndex}`} map={textures.normal} side={materialSide} />
                : viewMode === 'specularMap' && textures.specular
                    ? <meshBasicMaterial key={`specular-${uvIndex}`} map={textures.specular} side={materialSide} />
                : viewMode === 'metalnessMap' && textures.metalness
                    ? <meshBasicMaterial key={`metalness-${uvIndex}`} map={textures.metalness} side={materialSide} />
                : viewMode === 'roughnessMap' && textures.roughness
                    ? <meshBasicMaterial key={`roughness-${uvIndex}`} map={textures.roughness} side={materialSide} />
                : viewMode === 'emissionMap' && textures.emission
                    ? <meshBasicMaterial key={`emission-${uvIndex}`} map={textures.emission} side={materialSide} />
                : viewMode === 'diffuse' && textures.base
                    ? <meshBasicMaterial key={`diffuse-${uvIndex}`} map={textures.base} side={materialSide} transparent alphaTest={0.02} />
                : celShading && ['default', 'lighting', 'wireframe'].includes(viewMode)
                    ? <meshToonMaterial key={`cel-${viewMode}-glow-${glow}`} color={surfaceColor} map={textures.base} normalMap={textures.normal} gradientMap={celGradient} alphaMap={textures.mask} emissiveMap={glow && viewMode === 'default' ? textures.emission : null} emissive={glow && viewMode === 'default' && textures.emission ? '#ffffff' : '#000000'} vertexColors={!textures.base && mesh.use_vertex_colors} wireframe={viewMode === 'wireframe'} side={materialSide} transparent={Boolean(textures.mask || textures.base) || (mesh.material_opacity ?? 1) < 1} opacity={mesh.material_opacity ?? 1} alphaTest={textures.mask ? 0.2 : textures.base ? 0.02 : 0} />
                : ['default', 'lighting', 'wireframe'].includes(viewMode)
                    ? <meshPhysicalMaterial key={`${viewMode}-${uvIndex}-glow-${glow}`} color={surfaceColor} map={textures.base} normalMap={textures.normal} roughnessMap={textures.roughness} metalnessMap={textures.metalness} alphaMap={textures.mask} aoMap={textures.ambientOcclusion} aoMapIntensity={0.35} specularColorMap={textures.specular} emissiveMap={glow && viewMode === 'default' ? textures.emission : null} emissive={glow && viewMode === 'default' && textures.emission ? '#ffffff' : '#000000'} vertexColors={!textures.base && mesh.use_vertex_colors} wireframe={viewMode === 'wireframe'} roughness={0.72} metalness={viewMode === 'lighting' ? 0 : 0.05} side={materialSide} transparent={Boolean(textures.mask || textures.base) || (mesh.material_opacity ?? 1) < 1} opacity={mesh.material_opacity ?? 1} alphaTest={textures.mask ? 0.2 : textures.base ? 0.02 : 0} />
                    : <meshBasicMaterial vertexColors side={materialSide} />}
        </mesh>
        {viewMode === 'default' && weightBone >= 0 && !mesh.hidden && <mesh geometry={geometry} renderOrder={2}>
            <meshBasicMaterial vertexColors transparent opacity={0.8} blending={THREE.AdditiveBlending} depthWrite={false} side={materialSide} />
        </mesh>}
        {mesh.selected && !mesh.hidden && ['default', 'diffuse'].includes(viewMode) && <mesh geometry={geometry} renderOrder={19}>
            <meshBasicMaterial color="#00d9ff" transparent opacity={0.22} depthTest={false} depthWrite={false} side={materialSide} />
        </mesh>}
        {mesh.selected && !mesh.hidden && <lineSegments geometry={selectedEdges} renderOrder={20}><lineBasicMaterial color="#00e5ff" depthTest={false} transparent opacity={1} /></lineSegments>}
        {showNormals && <lineSegments geometry={normalLines}><lineBasicMaterial color="#55e6ff" depthTest={false} transparent opacity={0.8} /></lineSegments>}
    </group>;
}

function Skeleton({ bones, scaleMode, animation, animationWorlds }) {
    const points = useMemo(() => {
        const worlds = boneWorldMatrices(bones, scaleMode);
        const values = [];
        bones.forEach((bone, index) => {
            if (bone.parent_index < 0 || !worlds[bone.parent_index]) return;
            values.push(...new THREE.Vector3().setFromMatrixPosition(worlds[bone.parent_index]).toArray());
            values.push(...new THREE.Vector3().setFromMatrixPosition(worlds[index]).toArray());
        });
        return new Float32Array(values);
    }, [bones, scaleMode]);
    const attributeRef = useRef();
    useEffect(() => {
        if (animation || !attributeRef.current) return;
        attributeRef.current.array.set(points);
        attributeRef.current.needsUpdate = true;
    }, [animation, points]);
    useFrame(() => {
        if (!animation || !attributeRef.current) return;
        const worlds = animationWorlds.current;
        let cursor = 0;
        bones.forEach((bone, index) => {
            if (bone.parent_index < 0 || !worlds[bone.parent_index]) return;
            new THREE.Vector3().setFromMatrixPosition(worlds[bone.parent_index]).toArray(points, cursor);
            cursor += 3;
            new THREE.Vector3().setFromMatrixPosition(worlds[index]).toArray(points, cursor);
            cursor += 3;
        });
        attributeRef.current.needsUpdate = true;
    });
    return <lineSegments><bufferGeometry><bufferAttribute ref={attributeRef} attach="attributes-position" args={[points, 3]} /></bufferGeometry><lineBasicMaterial color="#ffd166" depthTest={false} /></lineSegments>;
}

function SceneExposure({ brightness }) {
    const renderer = useThree((state) => state.gl);
    useEffect(() => {
        renderer.toneMappingExposure = brightness;
        return () => { renderer.toneMappingExposure = 1; };
    }, [renderer, brightness]);
    return null;
}

function ViewportCapture({ captureRef }) {
    const { gl, scene, camera } = useThree();
    useEffect(() => {
        captureRef.current = () => {
            const background = scene.background;
            const clearColor = gl.getClearColor(new THREE.Color()).clone();
            const clearAlpha = gl.getClearAlpha();
            scene.background = null;
            gl.setClearColor(0x000000, 0);
            gl.render(scene, camera);
            const dataUrl = gl.domElement.toDataURL('image/png');
            scene.background = background;
            gl.setClearColor(clearColor, clearAlpha);
            gl.render(scene, camera);
            return dataUrl;
        };
        return () => {
            captureRef.current = null;
        };
    }, [camera, captureRef, gl, scene]);
    return null;
}

function BatchViewportCapture({ captureRef }) {
    const pendingRef = useRef(null);
    useEffect(() => {
        captureRef.current = () => new Promise((resolve) => {
            pendingRef.current = resolve;
        });
        return () => {
            captureRef.current = null;
            pendingRef.current?.(null);
            pendingRef.current = null;
        };
    }, [captureRef]);
    useFrame(({ gl, scene, camera }) => {
        const resolve = pendingRef.current;
        if (!resolve) return;
        pendingRef.current = null;
        const background = scene.background;
        const clearColor = gl.getClearColor(new THREE.Color()).clone();
        const clearAlpha = gl.getClearAlpha();
        scene.background = null;
        gl.setClearColor(0x000000, 0);
        gl.render(scene, camera);
        const dataUrl = gl.domElement.toDataURL('image/png');
        scene.background = background;
        gl.setClearColor(clearColor, clearAlpha);
        resolve(dataUrl);
    });
    return null;
}

function FrontCamera({ render, applyRigidTransform, onReady }) {
    const { camera, controls, size } = useThree();
    const bounds = useMemo(() => {
        const box = new THREE.Box3();
        const point = new THREE.Vector3();
        const worlds = boneWorldMatrices(render.bones || [], render.scale_mode);
        for (const mesh of render.meshes || []) {
            for (let index = 0; index < mesh.positions.length; index += 1) {
                point.fromArray(mesh.positions[index]);
                if (applyRigidTransform && mesh.vertex_skin_count === 1) {
                    const boneIndex = mesh.bone_indices[index]?.[0] ?? mesh.bone_index;
                    if (worlds[boneIndex]) point.applyMatrix4(worlds[boneIndex]);
                }
                box.expandByPoint(point);
            }
        }
        return box;
    }, [render, applyRigidTransform]);
    useEffect(() => {
        if (bounds.isEmpty()) return;
        const center = bounds.getCenter(new THREE.Vector3());
        const dimensions = bounds.getSize(new THREE.Vector3());
        const fov = THREE.MathUtils.degToRad(camera.fov || 42);
        const aspect = Math.max(size.width / Math.max(size.height, 1), 0.01);
        const distance = Math.max(
            dimensions.y / (2 * Math.tan(fov / 2)),
            dimensions.x / (2 * Math.tan(fov / 2) * aspect),
            dimensions.z,
            0.01,
        ) * 1.15;
        camera.up.set(0, 1, 0);
        camera.position.set(center.x, center.y, center.z + distance);
        camera.near = Math.max(distance / 10_000, 0.0001);
        camera.far = Math.max(distance * 100, 1000);
        camera.lookAt(center);
        camera.updateProjectionMatrix();
        if (controls) {
            controls.target.copy(center);
            controls.update();
        }
        onReady?.();
    }, [bounds, camera, controls, size.height, size.width, onReady]);
    return null;
}

function ResourceScene({ bfres, render, animation, animationPlaying = true, animationSeek, onAnimationTime, viewMode, uvIndex, brightness, celShading, glow, culling, showSkeleton, showNormals, weightBone, weightPreviewColors, selectedMesh, selectedMaterial, onSelectMesh, modelVisible, hiddenMeshes, cacheTextures = true, onCameraReady }) {
    const textures = useResolvedTextures(bfres?.resolvedTextures, cacheTextures);
    // G1M and LM3 store vertices already in model space. Baking a bone's world
    // matrix into them (the BFRES rigid-bind path) scatters the geometry.
    const applyRigidTransform = bfres?.format !== 'G1M' && bfres?.format !== 'LM3';
    const { restWorlds, worldsRef: animationWorlds } = useAnimationPose(render.bones, render.scale_mode, animation, animationPlaying, animationSeek, onAnimationTime);
    return <>
        <SceneExposure brightness={brightness} />
        <color attach="background" args={['#11151b']} />
        <ambientLight intensity={celShading ? 1.4 : 2.8} />
        {!celShading && <hemisphereLight args={['#ffffff', '#56616f', 2.0]} />}
        <directionalLight position={[6, 10, 8]} intensity={celShading ? 2.2 : 3.5} />
        <PerspectiveCamera makeDefault position={[0, 0, 10]} up={[0, 1, 0]} fov={42} />
        <OrbitControls makeDefault enableDamping dampingFactor={0.08} zoomSpeed={1.75} />
        <FrontCamera render={render} applyRigidTransform={applyRigidTransform} onReady={onCameraReady} />
        <Grid infiniteGrid fadeDistance={45} fadeStrength={4} cellColor="#33404d" sectionColor="#53687a" />
        <group visible={modelVisible}>{render.meshes.map((mesh, index) => <RenderMesh key={`${mesh.name}-${index}`} mesh={{ ...mesh, selected: mesh.name === selectedMesh || (selectedMaterial !== null && mesh.material_index === selectedMaterial), hidden: hiddenMeshes.includes(mesh.name) }} bones={render.bones} scaleMode={render.scale_mode} applyRigidTransform={applyRigidTransform} animation={animation} restWorlds={restWorlds} animationWorlds={animationWorlds} culling={culling} viewMode={viewMode} uvIndex={uvIndex} celShading={celShading} glow={glow} weightBone={weightBone} weightPreviewColors={weightPreviewColors?.[index]} showNormals={showNormals} onSelect={onSelectMesh} textures={materialTextures(bfres?.materials?.[mesh.material_index], textures, bfres?.format !== 'GLB')} />)}</group>
        {showSkeleton && <Skeleton bones={render.bones} scaleMode={render.scale_mode} animation={animation} animationWorlds={animationWorlds} />}
    </>;
}

function TreeFolder({ label, count, children }) {
    return <details open className="bfres-tree-folder">
        <summary><span>▾</span>{label}<small>{count}</small></summary>
        <div>{children}</div>
    </details>;
}

function Folder({ label, children, open = false, detail, onSelect, onContextMenu, checked, onToggle }) {
    return <details open={open} className="bfres-folder-node">
        <summary onClick={onSelect} onContextMenu={onContextMenu}><span className="bfres-folder-arrow">›</span>{checked !== undefined && <input type="checkbox" checked={checked} onClick={(event) => event.stopPropagation()} onChange={onToggle} />}<span className="bfres-folder-icon">■</span><strong>{label}</strong>{detail && <small>{detail}</small>}</summary>
        <div>{children}</div>
    </details>;
}

function ResourceTree({ bfres, title, onSection, onMesh, onBone, onModel, onContext, modelVisible, hiddenMeshes, onToggleModel, onToggleMesh }) {
    const natural = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });
    const naturally = (values, getName = (value) => value?.name || '') => [...values].sort((left, right) => natural.compare(getName(left), getName(right)));
    const sections = bfres?.sections || [];
    const materials = naturally(bfres?.materials || []);
    const textures = naturally(sections.filter((section) => ['FTXP', 'FTEX', 'BNTX'].includes(String.fromCharCode(...section.signature))));
    const meshes = naturally(bfres?.render?.meshes || []);
    const resolvedTextures = bfres?.resolvedTextures || [];
    const isG1m = bfres?.format === 'G1M';
    const embeddedTextures = resolvedTextures.filter((texture) => texture.source === 'embedded');
    const g1tTextures = isG1m ? resolvedTextures.filter((texture) => texture.source !== 'embedded') : [];
    const treeTextures = naturally([...embeddedTextures, ...g1tTextures]);
    const texToGoTextures = naturally(isG1m ? [] : resolvedTextures.filter((texture) => texture.source !== 'embedded'));
    const bones = bfres?.render?.bones || [];
    const animations = naturally(bfres?.animations || []);
    const node = (name, detail, action, key, kind) => <button type="button" className="bfres-tree-node" onClick={action} onContextMenu={(event) => onContext(event, kind, name)} key={key} title={name}>
        {kind === 'object' && <input type="checkbox" checked={!hiddenMeshes.includes(name)} onClick={(event) => event.stopPropagation()} onChange={() => onToggleMesh(name)} />}<span>{name || 'Unnamed'}</span><small>{detail}</small>
    </button>;
    const boneNodes = (parentIndex) => bones.map((bone, index) => ({ bone, index })).filter(({ bone }) => bone.parent_index === parentIndex).map(({ bone, index }) => {
        const children = boneNodes(index);
        if (children.length === 0) return node(bone.name, '', () => onBone(bone, index), `bone-${index}`);
        return <details className="bfres-bone-branch" key={`bone-${index}`}>
            <summary onClick={() => onBone(bone, index)}><span>▸</span><strong>{bone.name}</strong><small>{children.length}</small></summary>
            <div>{children}</div>
        </details>;
    });
    const modelName = sections.find((section) => String.fromCharCode(...section.signature) === 'FMDL')?.name || bfres?.name || 'Model';
    return <nav className="bfres-resource-tree" aria-label="3D resources">
        <div className="bfres-tree-actions"><button type="button" title="Expand resources">＋</button><span>Resources</span></div>
        <Folder label={title || bfres?.name || 'BFRES'} open>
            <Folder label="Models" open detail="1">
                <Folder label={modelName} checked={modelVisible} onToggle={onToggleModel} onSelect={() => onModel(modelName)} onContextMenu={(event) => onContext(event, 'model', modelName)}>
                    <Folder label="Objects" detail={meshes.length}>{meshes.map((mesh, index) => node(mesh.name, ``, () => onMesh(mesh), `mesh-${index}`, 'object'))}</Folder>
                    {/* <Folder label="Objects" open detail={meshes.length}>{meshes.map((mesh, index) => node(mesh.name, `${mesh.positions.length} vertices`, () => onMesh(mesh), `mesh-${index}`, 'object'))}</Folder> */}
                    <Folder label="Materials" detail={materials.length}>{materials.map((material, index) => node(material.name, ``, () => onSection(material, 'material'), `material-${material.offset}-${index}`, 'material'))}</Folder>
                    {/* <Folder label="Materials" open detail={materials.length}>{materials.map((material) => node(material.name, `${material.texture_slots.length} textures`, () => onSection(material, 'material'), `material-${material.offset}`, 'material'))}</Folder> */}
                    <Folder label="Skeleton" detail={bones.length}>{boneNodes(-1)}</Folder>
                </Folder>
            </Folder>
            <Folder label="Textures" detail={textures.length + treeTextures.length}>
                {textures.map((section) => node(section.name, 'Texture', () => onSection(section), `texture-${section.offset}`))}
                {treeTextures.map((texture) => node(texture.name, `${texture.width} × ${texture.height}`, () => onSection(texture, 'texture'), `resolved-texture-${texture.path}`))}
            </Folder>
            <Folder label="Animations" detail={animations.length + (bfres?.sections || []).filter((section) => ['FSKA', 'FSHU', 'FSHA', 'FVIS', 'FMAA'].includes(String.fromCharCode(...section.signature))).length}>
                {animations.map((animation, index) => node(animation.name, `${animation.duration?.toFixed?.(2) || 0}s`, () => onSection(animation, 'animation'), `animation-${index}`, 'animation'))}
            </Folder>
            <Folder label="Embedded Files" />
            {!isG1m && <Folder label="TexToGo" detail={texToGoTextures.length}>{texToGoTextures.map((texture) => node(texture.name, `${texture.width} × ${texture.height}`, () => onSection(texture, 'texture'), `textogo-${texture.name}`))}</Folder>}
        </Folder>
    </nav>;
}

function PropertyValue({ value }) {
    if (value === null || value === undefined) return <span className="bfres-null">null</span>;
    if (Array.isArray(value)) {
        const simple = value.length <= 8 && value.every((item) => typeof item !== 'object');
        if (simple) return <code>[{value.join(', ')}]</code>;
        return <details className="bfres-property-group"><summary>{value.length} items</summary><pre>{JSON.stringify(value, null, 2)}</pre></details>;
    }
    if (typeof value === 'object') return <details className="bfres-property-group" open><summary>{Object.keys(value).length} properties</summary><PropertyList value={value} /></details>;
    return <code>{String(value)}</code>;
}

function PropertyList({ value }) {
    return <dl className="bfres-property-list">
        {Object.entries(value || {}).map(([name, property]) => <div key={name}><dt>{name}</dt><dd><PropertyValue value={property} /></dd></div>)}
    </dl>;
}

function NodeInspector({ detail, textures }) {
    if (!detail) return <div className="bfres-empty-detail">Select a node in the scene collection to inspect its parsed properties.</div>;
    if (detail.type === 'model') return <ModelInspector model={detail.value} />;
    if (detail.type === 'mesh') return <MeshInspector mesh={detail.value} />;
    if (detail.type === 'material') return <MaterialInspector material={detail.value} textures={textures} />;
    if (detail.type === 'texture') return <section className="bfres-selected-detail bfres-texture-preview">
        <header><strong>{detail.value.name || 'Texture'}</strong><small>TEXTURE</small></header>
        {detail.value.dataUrl && <img src={detail.value.dataUrl} alt={detail.value.name || 'Texture preview'} />}
        <PropertyList value={{ width: detail.value.width, height: detail.value.height, path: detail.value.path }} />
    </section>;
    return <section className="bfres-selected-detail">
        <header><strong>{detail.value.name || 'Unnamed'}</strong><small>{detail.type}</small></header>
        <PropertyList value={detail.value} />
    </section>;
}

function InspectorTabs({ tabs, active, setActive }) {
    return <div className="bfres-inspector-tabs">{tabs.map((tab) => <button type="button" key={tab} className={active === tab ? 'active' : ''} onClick={() => setActive(tab)}>{tab}</button>)}</div>;
}

function ModelInspector({ model }) {
    const [tab, setTab] = useState('Sub Section');
    return <section className="bfres-selected-detail bfres-special-inspector"><InspectorTabs tabs={['Sub Section', 'User Data']} active={tab} setActive={setTab} />
        {tab === 'Sub Section' ? <><header><strong>Model</strong><small>FMDL</small></header><PropertyList value={model} /></> : <div className="bfres-empty-detail">No model user data was decoded.</div>}
    </section>;
}

function MeshInspector({ mesh }) {
    const [tab, setTab] = useState('Geometry');
    const material = mesh.material_name || `Material ${mesh.material_index}`;
    return <section className="bfres-selected-detail bfres-special-inspector"><header><strong>{mesh.name}</strong><small>OBJECT / SUBMESH</small></header>
        <div className="bfres-form-grid"><label>Name<input value={mesh.name} readOnly /></label><label className="bfres-check"><input type="checkbox" defaultChecked />Visible</label><label>Material<input value={material} readOnly /></label></div>
        <InspectorTabs tabs={['Geometry', 'Level of Detail', 'UVs', 'Normals', 'Colors', 'Skinning']} active={tab} setActive={setTab} />
        <PropertyList value={tab === 'Geometry' ? { vertex_count: mesh.positions.length, index_count: mesh.indices.length, triangle_count: Math.floor(mesh.indices.length / 3), material_index: mesh.material_index, bone_index: mesh.bone_index } : tab === 'Level of Detail' ? { level: 0, index_count: mesh.indices.length, primitive: 'Triangles' } : tab === 'UVs' ? { layers: mesh.uv0?.length ? 1 : 0, coordinates: mesh.uv0?.length || 0 } : tab === 'Normals' ? { count: mesh.normals?.length || 0 } : tab === 'Colors' ? { layers: mesh.colors?.length ? 1 : 0, values: mesh.colors?.length || 0 } : { vertex_skin_count: mesh.vertex_skin_count, skin_bones: mesh.skin_bones }} />
        <div className="bfres-action-grid"><button type="button">Export</button><button type="button">Replace (Static)</button><button type="button">Recalculate Tangents/Bitangents</button><button type="button">Open Material Editor</button></div>
    </section>;
}

function MaterialInspector({ material, textures }) {
    const [tab, setTab] = useState('Textures');
    const [selectedSlot, setSelectedSlot] = useState(null);
    useEffect(() => setSelectedSlot(null), [material]);
    const preview = selectedSlot
        ? (textures || []).find((texture) => texture.name === selectedSlot.name
            || texture.aliases?.includes(selectedSlot.name))
        : null;
    return <section className="bfres-selected-detail bfres-special-inspector"><header><strong>{material.name}</strong><small>MATERIAL</small></header>
        <div className="bfres-form-grid"><label>Name<input value={material.name} readOnly /></label><label className="bfres-check"><input type="checkbox" defaultChecked />Visible</label><label>Shader Archive<input value="material" readOnly /></label><label>Shader Model<input value="material" readOnly /></label><label>Sampler Inputs<input value={material.texture_slots.length} readOnly /></label><label>Attribute Inputs<input value="—" readOnly /></label></div>
        <InspectorTabs tabs={['Textures', 'Parameters', 'Render Info', 'Shader Options', 'User Data']} active={tab} setActive={setTab} />
        {tab === 'Textures' ? <><table className="bfres-texture-table"><thead><tr><th>Texture</th><th>Type</th><th>Sampler</th></tr></thead><tbody>{material.texture_slots.map((slot) => <tr key={slot.index} className={selectedSlot?.index === slot.index ? 'selected' : ''} onClick={() => setSelectedSlot(slot)}><td>{slot.name}</td><td>{slot.texture_type}</td><td>{slot.sampler || '—'}</td></tr>)}</tbody></table><div className="bfres-action-grid">
            {/* <button type="button">Add</button>
            <button type="button">Remove</button>
            <button type="button">Edit</button> */}
            </div>{selectedSlot && <div className="bfres-material-texture-preview">{preview?.dataUrl ? <img src={preview.dataUrl} alt={`${selectedSlot.name} preview`} /> : <span>Preview unavailable</span>}</div>}</> : <div className="bfres-empty-detail">No decoded {tab.toLowerCase()} entries.</div>}
    </section>;
}

function ResourceContextMenu({ menu, close, action }) {
    if (!menu) return null;
    const common = menu.kind === 'model' ? ['Export', 'Replace', 'Rename', 'Delete', 'Transform', 'Calculate Tangents/Bitangents', 'Normals', 'UVs', 'Colors', 'Collapse All', 'Expand All'] : menu.kind === 'material' ? ['Export', 'Replace', 'Copy', 'Rename', 'Delete'] : ['Export', 'Replace (Static)', 'Rename', 'Level Of Detail', 'Boundings', 'UVs', 'Normals', 'Colors', 'Recalculate Tangents/Bitangents', 'Fill Tangent Space with constant', 'Fill Bitangent Space with constant', 'Open Material Editor', 'Delete'];
    return <div className="bfres-resource-menu" style={{ left: menu.x, top: menu.y }} onMouseLeave={close}>{common.map((label) => <button type="button" key={label} onClick={() => { action(label, menu); close(); }}>{label}</button>)}</div>;
}

export default function Bfres3DView({ activeTab, setStatusText }) {
    const { documents, activeDocumentId } = useSyncExternalStore(subscribeDocuments, getDocumentsSnapshot);
    const document = documents.find((item) => item.id === activeDocumentId);
    const modelPathsKey = document?.modelPaths?.join('|') || '';
    const [bfres, setBfres] = useState(null);
    const [error, setError] = useState('');
    const [selected, setSelected] = useState(null);
    const [yaml, setYaml] = useState('');
    const [panel, setPanel] = useState('resources');
    const [viewMode, setViewMode] = useState('default');
    const [celShading, setCelShading] = useState(true);
    const [culling, setCulling] = useState(true);
    const [glow, setGlow] = useState(false);
    const [uvIndex, setUvIndex] = useState(0);
    const [brightness, setBrightness] = useState(1.0);
    const [brightnessLoaded, setBrightnessLoaded] = useState(false);
    const [showSkeleton, setShowSkeleton] = useState(true);
    const [showNormals, setShowNormals] = useState(false);
    const [weightBone, setWeightBone] = useState(-2);
    const [weightPreviewColors, setWeightPreviewColors] = useState(null);
    const [detail, setDetail] = useState(null);
    const [showEditor, setShowEditor] = useState(false);
    const [selectedMesh, setSelectedMesh] = useState('');
    const [selectedMaterial, setSelectedMaterial] = useState(null);
    const [leftWidth, setLeftWidth] = useState(240);
    const [rightWidth, setRightWidth] = useState(390);
    const [contextMenu, setContextMenu] = useState(null);
    const [modelVisible, setModelVisible] = useState(true);
    const [hiddenMeshes, setHiddenMeshes] = useState([]);
    const [viewResetKey, setViewResetKey] = useState(0);
    const [animationResetKey, setAnimationResetKey] = useState(0);
    const [fbxTextureFormat, setFbxTextureFormat] = useState('png');
    const [exportingModel, setExportingModel] = useState(false);
    const [replacingModel, setReplacingModel] = useState(false);

    useEffect(() => {
        const purge = (event) => {
            const paths = (event.detail?.paths || []).filter(Boolean);
            const keys = paths.map((path) => path.replace(/\\/g, '/').toLowerCase());
            keys.forEach((key) => modelInspectionCache.delete(key));
            if (keys.length > 1) modelInspectionCache.delete(keys.join('|'));
        };
        window.addEventListener('totkbits:model-cache-purge', purge);
        return () => window.removeEventListener('totkbits:model-cache-purge', purge);
    }, []);
    const [renderingViewport, setRenderingViewport] = useState(false);
    const captureViewportRef = useRef(null);
    const batchCaptureRef = useRef(null);
    const batchCameraReadyRef = useRef(null);
    const batchRunningRef = useRef(false);
    const [batchActive, setBatchActive] = useState(false);
    const [batchModel, setBatchModel] = useState(null);
    const [g1aAnimations, setG1aAnimations] = useState([]);
    const [selectedG1aPath, setSelectedG1aPath] = useState('');
    const [parsedG1aAnimations, setParsedG1aAnimations] = useState([]);
    const [g1aFailures, setG1aFailures] = useState({});
    const [loadedG1a, setLoadedG1a] = useState(null);
    const [g1aPlaying, setG1aPlaying] = useState(true);
    const [g1aPosition, setG1aPosition] = useState(0);
    const [g1aSeekRevision, setG1aSeekRevision] = useState(0);
    const hasActiveG1aPose = Boolean(loadedG1a);
    const [importingAllG1a, setImportingAllG1a] = useState(false);
    const [loadingG1aPath, setLoadingG1aPath] = useState('');
    const signalBatchCameraReady = useCallback(() => {
        const resolve = batchCameraReadyRef.current;
        batchCameraReadyRef.current = null;
        resolve?.();
    }, []);
    const isG1m = bfres?.format === 'G1M';
    const isGlb = bfres?.format === 'GLB';
    const isLm3 = bfres?.format === 'LM3';
    const hasGlow = useMemo(() => {
        const renderableTextures = new Set((bfres?.resolvedTextures || [])
            .filter((texture) => texture.renderable !== false)
            .flatMap((texture) => [texture.name, ...(texture.aliases || [])]));
        return (bfres?.materials || []).some((material) =>
            (material.texture_slots || []).some((slot) =>
                slot.texture_type === 'Emission' && renderableTextures.has(slot.name)));
    }, [bfres]);
    const hasSkeleton = (bfres?.render?.bones?.length || 0) > 0;
    const hasMeshes = (bfres?.render?.meshes?.length || 0) > 0;

    useEffect(() => {
        setWeightPreviewColors(null);
    }, [bfres]);


    useEffect(() => {
        if (viewMode !== 'weightsPrev' || !bfres?.render || !hasSkeleton || !hasMeshes || weightPreviewColors) return undefined;
        let cancelled = false;
        const operationId = `weights-preview:${document?.id || 'model'}:${crypto.randomUUID()}`;
        window.dispatchEvent(new CustomEvent('totkbits:model-loading', {
            detail: { id: operationId, label: 'Generating weight preview…' },
        }));
        const generate = async () => {
            await new Promise((resolve) => requestAnimationFrame(resolve));
            const colors = buildWeightPreview(bfres.render);
            if (!cancelled) setWeightPreviewColors(colors);
            await new Promise((resolve) => requestAnimationFrame(resolve));
            window.dispatchEvent(new CustomEvent('totkbits:model-loading', {
                detail: { id: operationId, done: true },
            }));
        };
        generate();
        return () => {
            cancelled = true;
            window.dispatchEvent(new CustomEvent('totkbits:model-loading', {
                detail: { id: operationId, done: true },
            }));
        };
    }, [viewMode, bfres, hasSkeleton, hasMeshes, weightPreviewColors, document?.id]);

    useEffect(() => {
        const clearAocModels = () => clearModelCaches();
        window.addEventListener('totkbits:aoc-config-changed', clearAocModels);
        return () => window.removeEventListener('totkbits:aoc-config-changed', clearAocModels);
    }, []);

    useEffect(() => {
        invoke('get_viewport_brightness')
            .then((saved) => setBrightness(Math.min(3, Math.max(0.3, Number(saved) || 1.0))))
            .catch(() => setBrightness(1.0))
            .finally(() => setBrightnessLoaded(true));
    }, []);

    useEffect(() => {
        if (!brightnessLoaded) return undefined;
        const timeout = window.setTimeout(() => {
            invoke('set_viewport_brightness', { brightness }).catch((saveError) => {
                console.error('Unable to save 3D viewport brightness:', saveError);
            });
        }, 300);
        return () => window.clearTimeout(timeout);
    }, [brightness, brightnessLoaded]);

    useEffect(() => {
        const receive = async (event) => {
            if (batchRunningRef.current) {
                setStatusText('A batch render is already running');
                return;
            }
            const { sourceRoot, outputRoot } = event.detail || {};
            if (!sourceRoot || !outputRoot) return;
            batchRunningRef.current = true;
            setBatchActive(true);
            const operationId = `batch-render:${crypto.randomUUID()}`;
            let rendered = 0;
            let failed = 0;
            const timedOut = [];
            const updateProgress = (label, progress) => window.dispatchEvent(new CustomEvent('totkbits:model-loading', {
                detail: { id: operationId, label, progress },
            }));
            updateProgress('Finding 3D files…', 0);
            // Let the overlay paint before starting the first native parse.
            await new Promise((resolve) => requestAnimationFrame(resolve));
            try {
                const files = await invoke('list_batch_render_files', {
                    sourceRoot,
                    outputRoot,
                    existingPng: event.detail.existingPng,
                    modelKind: event.detail.modelKind,
                });
                if (!files.length) {
                    setStatusText(`No supported 3D files found in ${sourceRoot}`);
                    return;
                }
                for (let index = 0; index < files.length; index += 1) {
                    const file = files[index];
                    const name = file.source.replace(/\\/g, '/').split('/').pop();
                    updateProgress(`Rendering ${index + 1} of ${files.length}: ${name}`, (index / files.length) * 100);
                    await new Promise((resolve) => requestAnimationFrame(resolve));
                    try {
                        let value;
                        if (file.modelKind === 'g1m') {
                            const inspection = await invoke('inspect_batch_g1m', { path: file.source });
                            if (inspection.status === 'timeout') {
                                timedOut.push(file.source);
                                console.warn(`Batch render timed out after 60 seconds: ${file.source}`);
                                throw new Error('G1M parsing timed out after 60 seconds');
                            }
                            if (inspection.status !== 'ok' || !inspection.model) {
                                throw new Error(inspection.error || 'G1M worker failed');
                            }
                            value = inspection.model;
                        } else if (file.modelKind === 'glb') {
                            value = await inspectGlbPath(file.source);
                        } else {
                            value = await invoke('inspect_3d_model', { path: file.source });
                        }
                        if (!value?.render?.meshes?.length) throw new Error('model contains no renderable meshes');
                        // Decode embedded texture images before committing the scene, otherwise
                        // TextureLoader can still be pending when the first frame is captured.
                        const urls = (value.resolvedTextures || []).flatMap((texture) =>
                            texture.dataUrls?.length ? texture.dataUrls : [texture.dataUrl]).filter(Boolean);
                        await Promise.all(urls.map((url) => new Promise((resolve) => {
                            const image = new Image();
                            image.onload = resolve;
                            image.onerror = resolve;
                            image.src = url;
                        })));
                        const cameraReady = new Promise((resolve) => {
                            batchCameraReadyRef.current = resolve;
                        });
                        setBatchModel(value);
                        let cameraPositioned = false;
                        cameraReady.then(() => { cameraPositioned = true; });
                        for (let frame = 0; !cameraPositioned && frame < 120; frame += 1) {
                            await new Promise((resolve) => requestAnimationFrame(resolve));
                        }
                        if (!cameraPositioned) throw new Error('batch camera did not finish positioning');
                        // Allow the fitted camera and updated controls to be used by two
                        // complete renderer frames before asking for the capture frame.
                        await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
                        let requestCapture = batchCaptureRef.current;
                        for (let frame = 0; !requestCapture && frame < 60; frame += 1) {
                            await new Promise((resolve) => requestAnimationFrame(resolve));
                            requestCapture = batchCaptureRef.current;
                        }
                        if (!requestCapture) throw new Error('batch viewport is not ready');
                        const dataUrl = await requestCapture();
                        if (!dataUrl) throw new Error('batch viewport closed before capture');
                        await invoke('export_viewport_png', { output: file.output, dataUrl });
                        rendered += 1;
                    } catch (reason) {
                        failed += 1;
                        console.error(`Batch render failed for ${file.source}:`, reason);
                    }
                    // Fully unmount this transient scene before parsing the next file.
                    // Its geometries/materials are released by React Three Fiber and its
                    // batch-only textures are disposed by useResolvedTextures cleanup.
                    setBatchModel(null);
                    batchCameraReadyRef.current = null;
                    await new Promise((resolve) => requestAnimationFrame(resolve));
                    updateProgress(`Rendered ${rendered} of ${files.length}${failed ? ` (${failed} failed)` : ''}`, ((index + 1) / files.length) * 100);
                    await new Promise((resolve) => requestAnimationFrame(resolve));
                }
                if (timedOut.length) console.warn('Batch render timed-out G1M files:', timedOut);
                setStatusText(`Batch render complete: ${rendered} PNG${rendered === 1 ? '' : 's'}${failed ? `, ${failed} failed` : ''}${timedOut.length ? `, ${timedOut.length} timed out` : ''} → ${outputRoot}`);
            } catch (reason) {
                setStatusText(`Batch render failed: ${reason}`);
            } finally {
                setBatchModel(null);
                setBatchActive(false);
                batchRunningRef.current = false;
                window.dispatchEvent(new CustomEvent('totkbits:model-loading', {
                    detail: { id: operationId, done: true },
                }));
            }
        };
        window.addEventListener('totkbits:batch-render', receive);
        return () => window.removeEventListener('totkbits:batch-render', receive);
    }, [brightness, celShading, culling, glow, showSkeleton, showNormals, setStatusText]);

    useEffect(() => {
        if (activeTab !== '3D' || !document?.fullPath) return;
        setSelectedMesh('');
        setSelectedMaterial(null);
        setGlow(false);
        // The same read-only model may be opened in more than one document.
        // Cache by canonical-looking path, not tab id, so all tabs share the
        // complete parsed model and its resolved image payloads.
        const modelPaths = document.modelPaths?.length ? document.modelPaths : [document.fullPath];
        const cacheKey = modelPaths.map((path) => path.replace(/\\/g, '/').toLowerCase()).join('|');
        const cached = modelInspectionCache.get(cacheKey);
        const cachedG1mHasTextures = cached?.format !== 'G1M' || cached?.resolvedTextures?.length > 0;
        if (cached && cachedG1mHasTextures) {
            setBfres(cached);
            setViewMode('default');
            setError('');
            const statusTimeout = window.setTimeout(() => {
                if (cached.format === 'G1M') setStatusText('Done');
                else if (cached.materials) setStatusText(modelTextureStatus(cached));
            }, 0);
            return () => window.clearTimeout(statusTimeout);
        }
        let cancelled = false;
        const operationId = `model:${document.id}:${crypto.randomUUID()}`;
        const finishLoading = () => window.dispatchEvent(new CustomEvent('totkbits:model-loading', {
            detail: { id: operationId, done: true },
        }));
        setError('');
        setBfres(null);
        setModelVisible(true);
        setHiddenMeshes([]);
        window.dispatchEvent(new CustomEvent('totkbits:model-loading', {
            detail: { id: operationId, label: `Loading ${document.title || '3D model'}…` },
        }));
        const load = async () => {
            const importStarted = performance.now();
            // Give React and the browser a frame to display the overlay before parsing starts.
            await new Promise((resolve) => requestAnimationFrame(resolve));
            try {
                const inspected = await Promise.all(modelPaths.map(async (path) => {
                    const pathKey = path.replace(/\\/g, '/').toLowerCase();
                    const pathCached = modelInspectionCache.get(pathKey);
                    if (pathCached && hasCompleteTextureResolution(pathCached)) {
                        return { path, value: pathCached };
                    }
                    const value = document.fileType === 'GLB'
                        ? await inspectGlb(document.title, document.id)
                        : await invoke('inspect_3d_model', { path });
                    if (hasCompleteTextureResolution(value)) {
                        cacheModelInspection(pathKey, value);
                    }
                    return { path, value };
                }));
                const value = inspected.length > 1 ? mergeG1mModels(inspected) : inspected[0].value;
                if (cancelled) return;
                if (hasCompleteTextureResolution(value)) {
                    cacheModelInspection(cacheKey, value);
                }
                setBfres(value);
                setViewMode('default');
                if (value.format !== 'G1M' && value.materials) {
                    setStatusText(modelTextureStatus(value));
                }
                const initial = value.sections.find((section) => String.fromCharCode(...section.signature) === 'FMDL') || value.sections[0];
                setSelected(initial || null);
                setYaml(initial ? sectionYaml(initial) : '');
                // Keep the overlay over the potentially expensive Three.js scene commit.
                await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
                if (value.format === 'G1M') {
                    setStatusText(`Imported G1M in ${importDuration(performance.now() - importStarted)}`);
                } else if (value.format === 'LM3') {
                    const textures = value.resolvedTextures?.length || 0;
                    setStatusText(
                        `Parsed ${value.name || 'LM3 slot'} in ${importDuration(performance.now() - importStarted)}`
                        + ` — ${value.render?.meshes?.length || 0} meshes, ${value.render?.bones?.length || 0} bones,`
                        + ` ${textures} texture${textures === 1 ? '' : 's'}`,
                    );
                }
            } catch (reason) {
                if (!cancelled) setError(String(reason));
            } finally {
                finishLoading();
            }
        };
        load();
        return () => {
            cancelled = true;
            finishLoading();
        };
    }, [activeTab, document?.id, document?.fullPath, modelPathsKey]);

    const embeddedAnimations = useMemo(() => (bfres?.sections || []).filter((section) =>
        ['FSKA', 'FSHU', 'FSHA', 'FTXP', 'FVIS', 'FMAA'].includes(String.fromCharCode(...section.signature))), [bfres]);

    useEffect(() => {
        setG1aAnimations([]);
        setSelectedG1aPath('');
        setParsedG1aAnimations([]);
        setG1aFailures({});
        setLoadedG1a(null);
        if (!isG1m || !bfres?.model_hash) return undefined;
        let cancelled = false;
        invoke('list_g1a_animations', { modelHash: bfres.model_hash })
            .then((values) => {
                if (cancelled) return;
                const available = values || [];
                setG1aAnimations(available);
                setG1aFailures(Object.fromEntries(available.flatMap((animation) => {
                    const error = g1aInspectionFailures.get(animation.path);
                    return error ? [[animation.path, error]] : [];
                })));
                setParsedG1aAnimations(available.flatMap((animation) => {
                    const value = g1aInspectionCache.get(animation.path);
                    return value ? [{ ...animation, value, bound: bindG1aToModel(value, bfres) }] : [];
                }));
            })
            .catch((reason) => { if (!cancelled) setStatusText(`Unable to find G1A animations: ${reason}`); });
        return () => { cancelled = true; };
    }, [isG1m, bfres?.model_hash, setStatusText]);

    const loadG1a = async (animation, activate = true) => {
        setLoadingG1aPath(animation.path);
        setStatusText(`Loading G1A ${animation.name}…`);
        try {
            let value = g1aInspectionCache.get(animation.path);
            if (!value) {
                value = await invoke('inspect_g1a_animation', { path: animation.path });
                g1aInspectionCache.set(animation.path, value);
            }
            g1aInspectionFailures.delete(animation.path);
            setG1aFailures((current) => {
                const next = { ...current };
                delete next[animation.path];
                return next;
            });
            const bound = bindG1aToModel(value, bfres);
            const parsed = { ...animation, value, bound };
            setParsedG1aAnimations((current) => [
                ...current.filter((entry) => entry.path !== animation.path),
                parsed,
            ]);
            if (activate) {
                setLoadedG1a(parsed);
                setG1aPlaying(true);
                setG1aPosition(0);
                setDetail({ type: 'G1A', value: { header: value.header, ...bound } });
                setYaml(JSON.stringify({ header: value.header, ...bound }, null, 2));
                setStatusText(`Playing ${animation.name}: ${bound.tracks.length} mapped bones${bound.unmappedBoneIds.length ? `, ${bound.unmappedBoneIds.length} unmapped` : ''}`);
            }
            return true;
        } catch (reason) {
            const message = String(reason);
            g1aInspectionFailures.set(animation.path, message);
            setG1aFailures((current) => ({ ...current, [animation.path]: message }));
            setStatusText(`Unable to load G1A ${animation.name}: ${message}`);
            return false;
        } finally {
            setLoadingG1aPath('');
        }
    };

    const importAllG1a = async () => {
        setImportingAllG1a(true);
        let loaded = 0;
        for (const animation of g1aAnimations) {
            if (await loadG1a(animation, false)) loaded += 1;
        }
        setImportingAllG1a(false);
        setStatusText(`Imported ${loaded} of ${g1aAnimations.length} G1A animations`);
    };

    const importSelectedG1a = () => {
        const animation = g1aAnimations.find((entry) => entry.path === selectedG1aPath);
        if (animation) loadG1a(animation);
    };

    const selectParsedG1a = (animation) => {
        setLoadedG1a(animation);
        setG1aPlaying(true);
        setG1aPosition(0);
        setG1aSeekRevision((revision) => revision + 1);
        setDetail({ type: 'G1A', value: { header: animation.value.header, ...animation.bound } });
        setYaml(JSON.stringify({ header: animation.value.header, ...animation.bound }, null, 2));
        setStatusText(`Playing ${animation.name}`);
    };

    const resetAnimation = () => {
        if (!loadedG1a) return;
        setG1aPlaying(false);
        setG1aPosition(0);
        setG1aSeekRevision((revision) => revision + 1);
        setLoadedG1a(null);
        setAnimationResetKey((value) => value + 1);
        setStatusText(`Stopped ${loadedG1a.name} and returned to rest pose`);
    };

    const choose = (section) => {
        setSelectedMesh('');
        setSelectedMaterial(null);
        setSelected(section);
        setDetail({ type: String.fromCharCode(...section.signature), value: section });
        setYaml(sectionYaml(section));
    };
    const startPanelDrag = (side, event) => {
        event.preventDefault();
        const move = (moveEvent) => {
            if (side === 'left') setLeftWidth(Math.min(520, Math.max(170, moveEvent.clientX)));
            else setRightWidth(Math.min(680, Math.max(260, window.innerWidth - moveEvent.clientX)));
        };
        const stop = () => {
            window.removeEventListener('mousemove', move);
            window.removeEventListener('mouseup', stop);
        };
        window.addEventListener('mousemove', move);
        window.addEventListener('mouseup', stop);
    };
    const applyYaml = () => {
        setStatusText('BFRES node YAML is staged in the inspector; binary rebuilding is not available for this node type yet');
    };
    const showYamlButtonFlag = false;
    const showNormalsButtonFlag = false;
    const exportModel = async () => {
        if ((!isG1m && !isGlb && !isLm3) || !document?.fullPath || exportingModel) return;
        const sourcePaths = document.modelPaths?.length ? document.modelPaths : [document.fullPath];
        const stem = sourcePaths.length > 1
            ? 'selected_aoc_models'
            : (document.title || 'model').replace(/\.(g1m|glb)$/i, '').replace(/[^\w.-]+/g, '_');
        const output = await save({
            defaultPath: `${stem}.${isGlb ? 'glb' : 'fbx'}`,
            filters: isGlb ? [
                { name: 'Binary glTF model', extensions: ['glb'] },
            ] : [
                { name: 'FBX model', extensions: ['fbx'] },
                { name: 'Binary glTF model', extensions: ['glb'] },
            ],
        });
        if (!output) return;
        const extension = output.split('.').pop()?.toLowerCase();
        const allowedExtensions = isGlb ? ['glb'] : ['fbx', 'glb'];
        if (!allowedExtensions.includes(extension)) {
            setStatusText(isGlb ? 'GLB export requires a .glb filename' : 'Model export requires an .fbx or .glb filename');
            return;
        }
        const label = extension.toUpperCase();
        const operationId = `model-export:${document.id}:${crypto.randomUUID()}`;
        window.dispatchEvent(new CustomEvent('totkbits:model-loading', {
            detail: { id: operationId, label: `Exporting ${label} model…` },
        }));
        setExportingModel(true);
        setStatusText(`Exporting ${sourcePaths.length === 1 ? document.title || bfres.format : `${sourcePaths.length} G1M models`} as ${label}…`);
        try {
            const written = isGlb
                ? await invoke('export_loaded_glb', { documentId: document.id, output })
                : isLm3
                ? (extension === 'fbx'
                    ? await invoke('export_lm3_fbx', { documentId: document.id, sourcePath: document.fullPath, output, textureFormat: fbxTextureFormat })
                    : await invoke('export_lm3_glb', { documentId: document.id, sourcePath: document.fullPath, output }))
                : extension === 'fbx'
                ? await invoke('export_g1m_fbx', { documentId: document.id, sourcePaths, output, textureFormat: fbxTextureFormat })
                : await invoke('export_g1m_glb', { documentId: document.id, sourcePaths, output });
            setStatusText(`Exported ${label} ${written}`);
        } catch (reason) {
            setStatusText(`${label} export failed: ${reason}`);
        } finally {
            setExportingModel(false);
            window.dispatchEvent(new CustomEvent('totkbits:model-loading', {
                detail: { id: operationId, done: true },
            }));
        }
    };
    const replaceModelMeshes = async () => {
        if (!isG1m || !document?.fullPath || replacingModel) return;
        const fbx = await open({ multiple: false, filters: [{ name: 'FBX model', extensions: ['fbx'] }] });
        if (!fbx) return;
        const operationId = `mesh-replacement:${document.id}:${crypto.randomUUID()}`;
        window.dispatchEvent(new CustomEvent('totkbits:model-loading', {
            detail: { id: operationId, label: `Replacing meshes in ${document.title || 'G1M'}…` },
        }));
        setReplacingModel(true);
        setStatusText('Replacing G1M meshes…');
        try {
            await new Promise((resolve) => requestAnimationFrame(resolve));
            const value = await invoke('replace_g1m_meshes', { documentId: document.id, fbx });
            const modelPaths = document.modelPaths?.length ? document.modelPaths : [document.fullPath];
            const cacheKeys = modelPaths.map((path) => path.replace(/\\/g, '/').toLowerCase());
            if (cacheKeys.length === 1) cacheModelInspection(cacheKeys[0], value);
            else cacheModelInspection(cacheKeys.join('|'), value);
            setBfres(value);
            setSelectedMesh('');
            setSelectedMaterial(null);
            setHiddenMeshes([]);
            setViewResetKey((key) => key + 1);
            setStatusText('Meshes replaced. Use Save or Save As to write the G1M.');
        } catch (reason) {
            setStatusText(`Mesh replacement failed: ${reason}`);
        } finally {
            setReplacingModel(false);
            window.dispatchEvent(new CustomEvent('totkbits:model-loading', {
                detail: { id: operationId, done: true },
            }));
        }
    };
    const renderViewport = async () => {
        if (!captureViewportRef.current || renderingViewport) return;
        const stem = (document?.title || bfres?.name || 'model').replace(/\.[^.]+$/, '');
        const output = await save({
            defaultPath: `${stem}_render.png`,
            filters: [{ name: 'PNG image', extensions: ['png'] }],
        });
        if (!output) return;
        setRenderingViewport(true);
        setStatusText('Rendering transparent viewport PNG…');
        try {
            const dataUrl = captureViewportRef.current();
            await invoke('export_viewport_png', { output, dataUrl });
            setStatusText(`Rendered viewport PNG ${output}`);
        } catch (reason) {
            setStatusText(`Viewport render failed: ${reason}`);
        } finally {
            setRenderingViewport(false);
        }
    };

    return <>
        {batchActive && <div className="bfres-batch-viewport" aria-hidden="true">
            <Canvas dpr={1} gl={{ antialias: true, alpha: true, preserveDrawingBuffer: true }}>
                <BatchViewportCapture captureRef={batchCaptureRef} />
                {batchModel?.render && <ResourceScene bfres={batchModel} render={batchModel.render} viewMode="default" uvIndex={0} brightness={brightness} celShading={celShading} glow={glow} culling={culling} showSkeleton={showSkeleton} showNormals={showNormals} weightBone={-2} weightPreviewColors={null} selectedMesh="" selectedMaterial={null} modelVisible hiddenMeshes={[]} onSelectMesh={() => {}} cacheTextures={false} onCameraReady={signalBatchCameraReady} />}
            </Canvas>
        </div>}
        <main className="bfres-workspace" aria-hidden={activeTab !== '3D'} style={{ '--bfres-left-width': `${leftWidth}px`, '--bfres-right-width': `${rightWidth}px`, display: activeTab === '3D' ? 'grid' : 'none' }}>
        <header className="bfres-viewport-toolbar">
            <div className="bfres-toolbar-row bfres-toolbar-tabs">
                <button type="button" onClick={() => setPanel('resources')} className={panel === 'resources' ? 'active' : ''}>Resources</button>
                <button type="button" onClick={() => { if (!isG1m && !isGlb) setPanel('parameters'); }} className={panel === 'parameters' ? 'active' : ''} disabled={isG1m || isGlb}>Parameters</button>
                <button type="button" onClick={() => setPanel('animations')} className={panel === 'animations' ? 'active' : ''}>Animations <small>{embeddedAnimations.length + parsedG1aAnimations.length}</small></button>
                {hasGlow && <button type="button" onClick={() => setGlow((value) => !value)} className={glow ? 'active' : ''} aria-pressed={glow}>Glow</button>}
                <button type="button" onClick={() => setViewResetKey((value) => value + 1)}>Reset View</button>
                <button type="button" onClick={() => setCelShading((value) => !value)} className={celShading ? 'active' : ''}>Cel Shading</button>
            </div>
            <div className="bfres-toolbar-row bfres-toolbar-controls">
                <label className="bfres-shading-select">Shading:
                <select value={viewMode} onChange={(event) => { setViewMode(event.target.value); if (event.target.value === 'selectedBoneWeights' && weightBone < 0) setWeightBone(0); }}>
<option value="default">Default</option>
<option value="diffuse">Diffuse</option>
<option value="normalMap">NormalMap</option>
<option value="specularMap">SpecularMap</option>
<option value="selectedBoneWeights">Weights</option>
{hasSkeleton && hasMeshes && <option value="weightsPrev">WeightsPrev</option>}
<option value="emissionMap">EmissionMap</option>
<option value="normal">Normal</option>
{/* <option value="lighting">Lighting</option>
<option value="vertColor">VertColor</option>
<option value="ambientOcclusion">AmbientOcclusion</option>
<option value="uvCoords">UVCoords</option>
<option value="uvTestPattern">UVTestPattern</option>
<option value="tangents">Tangents</option>
<option value="bitangents">Bitangents</option>
<option value="lightMap">LightMap</option>
<option value="shadowMap">ShadowMap</option>
<option value="metalnessMap">MetalnessMap</option> */}
<option value="roughnessMap">RoughnessMap</option>
{/* <option value="subSurfaceScatteringMap">SubSurfaceScatteringMap</option> */}
<option value="wireframe">Wireframe</option>
<option value="blank">Blank</option>

                </select>
                </label>
                <label className="bfres-shading-select">UV map:
                <select value={uvIndex} onChange={(event) => setUvIndex(Number(event.target.value))}>
                    {Array.from({ length: Math.max(1, ...(bfres?.render?.meshes || []).map((mesh) => mesh.uv_maps?.length || (mesh.uv0?.length ? 1 : 0))) }, (_, index) => <option value={index} key={index}>UV {index}</option>
)}
                </select>
                </label>
                <label className="bfres-shading-select bfres-brightness">Brightness:
                <input type="range" min="0.3" max="3" step="0.1" value={brightness} onChange={(event) => setBrightness(Number(event.target.value))} aria-label="Viewport brightness" />
                <span>{Math.round((brightness/3) * 100)}%</span>
                </label>
                <button type="button" onClick={() => setShowSkeleton((value) => !value)} className={showSkeleton ? 'active' : ''}>Skeleton</button>
                {showNormalsButtonFlag && <button type="button" onClick={() => setShowNormals((value) => !value)} className={showNormals ? 'active' : ''}>Normals</button>}
                {/* <button type="button" onClick={() => setCelShading((value) => !value)} className={celShading ? 'active' : ''}>Cel Shading</button> */}
                {showYamlButtonFlag && <button type="button" onClick={() => setShowEditor((value) => !value)} className={!showEditor ? 'active' : ''}>{showEditor ? 'Hide YAML' : 'Show YAML'}</button>}
                {viewMode === 'selectedBoneWeights' && <select className="bfres-bone-select" value={weightBone} onChange={(event) => setWeightBone(Number(event.target.value))} aria-label="Selected bone weights">
                    {(bfres?.render?.bones || []).map((bone, index) => <option key={`${bone.name}-${index}`} value={index}>Bone: {bone.name}</option>
)}
                </select>}
            </div>
        </header>
        {error ? <div className="bfres-error">{error}</div> : <>
            <ResourceTree bfres={bfres} title={document?.title} modelVisible={modelVisible} hiddenMeshes={hiddenMeshes} onToggleModel={() => setModelVisible((value) => !value)} onToggleMesh={(name) => setHiddenMeshes((values) => values.includes(name) ? values.filter((value) => value !== name) : [...values, name])} onContext={(event, kind, name) => { event.preventDefault(); setContextMenu({ x: event.clientX, y: event.clientY, kind, name }); }} onModel={(name) => {
                setSelectedMesh('');
                setSelectedMaterial(null);
                setWeightBone(-2);
                const model = { name, path: document?.fullPath || '', vertex_buffer_count: bfres?.render?.meshes.length || 0, shape_count: bfres?.render?.meshes.length || 0, material_count: bfres?.materials?.length || 0, user_data_count: 0, total_vertex_count: (bfres?.render?.meshes || []).reduce((sum, mesh) => sum + mesh.positions.length, 0) };
                setDetail({ type: 'model', value: model });
                setYaml(JSON.stringify(model, null, 2));
                setStatusText(`Selected model ${name}`);
            }} onSection={(value, type) => {
                setSelectedMesh('');
                if (type === 'material') {
                    const materialIndex = (bfres?.materials || []).indexOf(value);
                    const usedBy = (bfres?.render?.meshes || []).filter((mesh) => mesh.material_index === materialIndex);
                    setSelectedMaterial(usedBy.length > 0 ? materialIndex : null);
                } else setSelectedMaterial(null);
                setWeightBone(-2);
                if (type === 'material' || type === 'texture' || type === 'animation') {
                    setSelected(value);
                    setDetail({ type, value });
                    setYaml(JSON.stringify(value, null, 2));
                    setStatusText(`Selected ${type} ${value.name}`);
                } else choose(value);
            }} onMesh={(mesh) => {
                setSelectedMesh(mesh.name);
                setSelectedMaterial(null);
                setWeightBone(-2);
                const inspectedMesh = { ...mesh, material_name: bfres?.materials?.[mesh.material_index]?.name };
                setDetail({ type: 'mesh', value: inspectedMesh });
                setYaml(JSON.stringify(mesh, null, 2));
                setStatusText(`Selected mesh ${mesh.name}`);
            }} onBone={(bone, index) => {
                setSelectedMesh('');
                setSelectedMaterial(null);
                setSelected(bone);
                setDetail({ type: 'bone', value: { index, ...bone } });
                setYaml(JSON.stringify({ index, ...bone }, null, 2));
                setWeightBone(index);
                setStatusText(`Selected bone ${bone.name}`);
            }} />
            <div className="bfres-panel-divider left" role="separator" aria-orientation="vertical" onMouseDown={(event) => startPanelDrag('left', event)} />
            <section className="bfres-viewport" aria-label="BFRES 3D viewport">
                <Canvas key={viewResetKey} dpr={[1, 2]} gl={{ antialias: true, alpha: true, preserveDrawingBuffer: true }} onPointerMissed={() => { setSelectedMesh(''); setSelectedMaterial(null); }}>
                    <ViewportCapture captureRef={captureViewportRef} />
                    {bfres?.render && <ResourceScene key={`animation-scene-${animationResetKey}`} bfres={bfres} render={bfres.render} animation={loadedG1a?.bound} animationPlaying={g1aPlaying} animationSeek={{ time: g1aPosition, revision: g1aSeekRevision }} onAnimationTime={setG1aPosition} viewMode={viewMode} uvIndex={uvIndex} brightness={brightness} celShading={celShading} glow={glow} culling={culling} showSkeleton={showSkeleton} showNormals={showNormals} weightBone={weightBone} weightPreviewColors={weightPreviewColors} selectedMesh={selectedMesh} selectedMaterial={selectedMaterial} modelVisible={modelVisible} hiddenMeshes={hiddenMeshes} onSelectMesh={(mesh) => {
                        setSelectedMesh(mesh.name);
                        setSelectedMaterial(null);
                        setWeightBone(-2);
                        setDetail({ type: 'mesh', value: mesh });
                        setYaml(JSON.stringify(mesh, null, 2));
                        setStatusText(`${mesh.name}: ${mesh.positions.length.toLocaleString()} vertices, ${(mesh.indices.length / 3).toLocaleString()} triangles`);
                    }} />}
                </Canvas>
                <div className="bfres-viewport-note">{bfres?.render?.meshes.length || 0} meshes · {(bfres?.render?.meshes || []).reduce((sum, mesh) => sum + mesh.positions.length, 0).toLocaleString()} vertices · {bfres?.render?.bones.length || 0} bones</div>
            </section>
            <div className="bfres-panel-divider right" role="separator" aria-orientation="vertical" onMouseDown={(event) => startPanelDrag('right', event)} />
            <aside className="bfres-inspector">
                {bfres?.render && <section className="bfres-export-panel">
                    <header><strong>Viewport</strong></header>
                    <button type="button" onClick={() => setCulling((value) => !value)} className={culling ? 'active' : ''} aria-pressed={culling}>Culling</button>
                    <button type="button" onClick={renderViewport} disabled={renderingViewport || !bfres.render.meshes?.length}>
                        {renderingViewport ? 'Rendering…' : 'Render'}
                    </button>
                </section>}
                <Tomodachi model={bfres} setModel={setBfres} documentPath={document?.fullPath} setStatusText={setStatusText} />
                {(isG1m || isGlb || isLm3) && <section className="bfres-export-panel">
                    {/* <header><strong>FBX Export</strong></header> */}
                    <button type="button" onClick={exportModel} disabled={exportingModel || !bfres?.render?.meshes?.length}>
                        {exportingModel ? 'Exporting…' : 'Export'}
                    </button>
                    {isG1m && <button type="button" onClick={replaceModelMeshes} disabled={replacingModel || exportingModel}>
                        {replacingModel ? 'Replacing…' : 'Replace meshes'}
                    </button>}
                    
                    {(isG1m || isLm3) && <><select value={fbxTextureFormat} onChange={(event) => setFbxTextureFormat(event.target.value)} disabled={exportingModel}>
                            <option value="none">None</option>
                            <option value="png">PNG</option>
                            <option value="dds">DDS</option>
                        </select>
<label>Textures
                    </label></>}
                    
                </section>}
                {panel === 'resources' && <NodeInspector detail={detail} textures={bfres?.resolvedTextures} />}
                {/* LM3 models carry no BFRES header, so the panel only renders
                    when the header actually exists. */}
                {panel === 'parameters' && bfres?.header && !isG1m && !isGlb && <dl className="bfres-parameters">
                    <dt>Version</dt><dd>{(bfres.header.version || []).join('.')}</dd>
                    <dt>Endian</dt><dd>{bfres.header.endian}</dd>
                    <dt>Address size</dt><dd>{bfres.header.target_address_size || 8} bytes</dd>
                    <dt>Alignment</dt><dd>2^{bfres.header.alignment_exponent}</dd>
                    <dt>Logical size</dt><dd>{bfres.header.file_size.toLocaleString()} bytes</dd>
                    <dt>String pool</dt><dd>0x{bfres.header.string_pool_offset.toString(16).toUpperCase()} · {bfres.header.string_pool_size.toLocaleString()} bytes</dd>
                    <dt>Relocation table</dt><dd>0x{bfres.header.relocation_table_offset.toString(16).toUpperCase()}</dd>
                    <dt>Sections</dt><dd>{bfres.sections.length}</dd>
                </dl>}
                {panel === 'animations' && <div className="bfres-animation-list">
                    {isG1m && g1aAnimations.length > 0 && <section className="bfres-export-panel">
                        <header><strong>Import G1A Animation</strong></header>
                        <select value={selectedG1aPath} onChange={(event) => setSelectedG1aPath(event.target.value)} disabled={Boolean(loadingG1aPath)} aria-label="G1A animation">
                            <option value="">Select animation…</option>
                            {g1aAnimations.map((animation) => <option key={animation.path} value={animation.path} style={{ color: g1aInspectionCache.has(animation.path) ? '#39d98a' : g1aFailures[animation.path] ? '#ff5c5c' : undefined }}>{animation.name}</option>)}
                        </select>
                        <button type="button" onClick={importSelectedG1a} disabled={!selectedG1aPath || Boolean(loadingG1aPath)}>{loadingG1aPath ? 'Importing…' : 'Import'}</button>
                        <button type="button" onClick={importAllG1a} disabled={importingAllG1a || Boolean(loadingG1aPath)}>{importingAllG1a ? 'Importing All…' : 'Import All'}</button>
                        {loadedG1a && <button type="button" onClick={() => setG1aPlaying((playing) => !playing)} className={g1aPlaying ? 'active' : ''}>{g1aPlaying ? 'Pause' : 'Play'}</button>}
                        {hasActiveG1aPose && <button type="button" onClick={resetAnimation} style={{ marginLeft: 'auto' }}>Reset</button>}
                        {selectedG1aPath && g1aFailures[selectedG1aPath] && <p role="alert" style={{ color: '#ff5c5c' }}>{g1aFailures[selectedG1aPath]}</p>}
                    </section>}
                    {embeddedAnimations.length === 0 && parsedG1aAnimations.length === 0 && <p>No animations have been imported.</p>}
                    {parsedG1aAnimations.map((animation) => <button type="button" key={animation.path} onClick={() => selectParsedG1a(animation)} className={loadedG1a?.path === animation.path ? 'active' : ''}>
                        <span>{loadedG1a?.path === animation.path && !g1aPlaying ? 'Ⅱ' : '▶'}</span><div><strong>{animation.name}</strong><small>G1A · cached{loadedG1a?.path === animation.path ? g1aPlaying ? ' and playing' : ' and paused' : ' and ready to play'}</small></div>
                    </button>)}
                    {embeddedAnimations.map((animation) => <button type="button" key={animation.offset} onClick={() => choose(animation)}>
                        <span>▶</span><div><strong>{animation.name || String.fromCharCode(...animation.signature)}</strong><small>{String.fromCharCode(...animation.signature)} · playback decoding pending</small></div>
                    </button>)}
                    {loadedG1a && <p>{loadedG1a.bound.duration.toFixed(2)}s · {loadedG1a.bound.tracks.length} mapped bones{loadedG1a.bound.unmappedBoneIds.length ? ` · ${loadedG1a.bound.unmappedBoneIds.length} unmapped` : ''} · version {loadedG1a.value.header.version}</p>}
                </div>}
                {showEditor && <section className="bfres-yaml-panel">
                    <header><strong>{selected?.name || 'Node YAML'}</strong><button type="button" onClick={applyYaml} disabled={!selected}>Stage YAML</button></header>
                    <Editor height="100%" language="yaml" theme="vs-dark" value={yaml} onChange={(value) => setYaml(value || '')} options={{ minimap: { enabled: false }, fontSize: 12, automaticLayout: true, scrollBeyondLastLine: false }} />
                </section>}
            </aside>
            {loadedG1a && <footer className="bfres-animation-timeline">
                <button type="button" onClick={() => setG1aPlaying((playing) => !playing)} aria-label={g1aPlaying ? 'Pause animation' : 'Play animation'}>{g1aPlaying ? 'Ⅱ' : '▶'}</button>
                <span>{playbackTime(g1aPosition)}</span>
                <input type="range" min="0" max={Math.max(loadedG1a.bound.duration, 0.001)} step="0.001" value={Math.min(g1aPosition, loadedG1a.bound.duration)} onChange={(event) => { setG1aPosition(Number(event.target.value)); setG1aSeekRevision((revision) => revision + 1); }} aria-label="Animation timeline" />
                <span>{playbackTime(loadedG1a.bound.duration)}</span>
                <strong>{loadedG1a.name}</strong>
            </footer>}
            <ResourceContextMenu menu={contextMenu} close={() => setContextMenu(null)} action={(label, menu) => setStatusText(`${label}: ${menu.name}`)} />
        </>}
        </main>
    </>;
}

