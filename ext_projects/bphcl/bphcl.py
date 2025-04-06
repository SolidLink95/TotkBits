from dataclasses import dataclass
import hashlib
import io
import os
from pathlib import Path
import struct
import sys
from BphclEnums import ResFileType, ResSectionType, ResTypeSectionSignature
from BphclSmallDataclasses import *
from Experimental import hclClothContainer, hkMemoryResourceContainer, hkaAnimationContainer
from Havok import hkRootLevelContainer
from Stream import ReadStream, WriteStream
import oead
import yaml
import json
from aamp_totk_hashes import AAMP_TOTK_HASHES
from util import DataConverter, _hex
from BphclMainDataclasses import ResPhive, ResTagFile

class Bphcl():
    def __init__(self):
        self.file:ResPhive = None
        self.tag_file:ResTagFile = None
        self.cloth_section = None
        self.aamp_hashes = AAMP_TOTK_HASHES
    
    # def remove_even_more_entries
    
    def cloth_section_to_binary(self) -> bytes:
        return oead.aamp.ParameterIO.to_binary(self.cloth_section)
    
    def cloth_section_to_dict(self) -> dict:
        """Very dirty approach, will fix later"""
        yaml_str = oead.aamp.ParameterIO.to_text(self.cloth_section)
        bad_tags = ["!io", "!list", "!obj"]
        for tag in bad_tags:
            yaml_str = yaml_str.replace(tag, "")
        for crc_hash, name in self.aamp_hashes.items():
            yaml_str = yaml_str.replace(crc_hash, name)
        yaml_dict = yaml.safe_load(yaml_str)
        return yaml_dict
    
    def to_dict(self) -> dict:
        self.validate()
        header = DataConverter.dataclass_to_dict(self.file)
        tag_file = DataConverter.dataclass_to_dict(self.tag_file)
        cloth_section = self.cloth_section_to_dict()
        result = {
            "header": header,
            "tag_file": tag_file,
            "cloth_section": cloth_section,
        }
        return result
    
    def to_yaml(self) -> str:
        data = self.to_dict()
        yaml_str = yaml.dump(data, default_flow_style=False, sort_keys=False)
        return yaml_str
    
    def to_json(self) -> str:
        data = self.to_dict()
        json_str = json.dumps(data, indent=4, default=str)
        return json_str
    
    def to_json_file(self, file_path: str) -> None:
        data = self.to_dict()
        with open(file_path, "w") as f:
            json.dump(data, f, indent=4, default=str)
    
    def to_yaml_file(self, file_path: str) -> None:
        data = self.to_dict()
        with open(file_path, "w") as f:
            yaml.dump(data, f, default_flow_style=False, sort_keys=False)
    
    def to_binary(self) -> bytes:
        result = self.file.to_binary()
        result += self.tag_file.to_binary()
        result += self.cloth_section_to_binary()
        return result
    
    def validate(self):
        if self.file is None:
            raise ValueError("ResPhive section not found or not read yet")
        if self.tag_file is None:
            raise ValueError("TAG0 section not found or not read yet")
        if self.cloth_section is None:
            raise ValueError("Cloth section not found or not read yet")
        self.file.validate()
        self.tag_file.validate()
    
    @staticmethod
    def from_file(file_path: str) -> "Bphcl":
        with open(file_path, "rb") as f:
            data = f.read()
        return Bphcl.from_binary(data)
    
    @staticmethod
    def from_binary(data: bytes) -> "Bphcl":
        stream = ReadStream(data)
        return Bphcl.from_reader(stream)
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "Bphcl":
        bphcl = Bphcl()
        bphcl.file = ResPhive.from_reader(stream)
        bphcl.tag_file = ResTagFile.from_reader(stream)
        aamp_signature = stream._read_exact(4)
        if aamp_signature != b"AAMP":
            raise ValueError(f"Invalid AAMP signature: {aamp_signature}, expected b'AAmp'")
        stream.seek(-4, io.SEEK_CUR)
        bphcl.cloth_section = oead.aamp.ParameterIO.from_binary(stream.read())
        bphcl.tag_file.update_strings()
        return bphcl


    def list_all_nodes(self) -> list[str]:
        _dict = self.cloth_section_to_dict()
        nodes = {"nodes": [], "collidables": []}
        tmp = _dict["param_root"]["lists"][1571872146]
        for k, v in _dict["param_root"]["lists"].items():
            if k == 1571872146:
                key = "nodes"
            elif k == 107719806:
                key = "collidables"
            else:
                continue
            i = 0
            for _i, _data in v["objects"].items():
                if "Name" in _data:
                    i += 1
                    nodes[key].append((i, _data["Name"]))
                else:
                    print(f"Key {_i} does not have a name")
            i = 0
            # nodes.append(_data["name"])
        
        return nodes
        
                