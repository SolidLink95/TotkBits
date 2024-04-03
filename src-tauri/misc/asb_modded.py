from exb import EXB
from utils import *
from enum import Enum
import zstd
import json
import os
import mmh3

# Reserialization does not support version 0x40F

class NodeType(Enum):
    FloatSelector           = 1
    StringSelector          = 2
    SkeletalAnimation       = 3
    State                   = 4
    Unknown2                = 5
    OneDimensionalBlender   = 6
    Sequential              = 7
    IntSelector             = 8
    Simultaneous            = 9
    Event                   = 10
    MaterialAnimation       = 11
    FrameController         = 12
    DummyAnimation          = 13
    RandomSelector          = 14
    Unknown4                = 15
    PreviousTagSelector     = 16
    BonePositionSelector    = 17
    BoneAnimation           = 18
    InitialFrame            = 19
    BoneBlender             = 20
    BoolSelector            = 21
    Alert                   = 22
    SubtractAnimation       = 23
    ShapeAnimation          = 24
    Unknown7                = 25

type_param = ["string", "int", "float", "bool", "vec3f", "userdefined"] # Data type order (blackboard parameters)

# Enums and stuff
class ASB:
    def __init__(self, data,from_json = False):
        # from_json = False
        if type(data) == str:
            if os.path.splitext(data)[1] == '.json':
                from_json = True
            if from_json:
                with open(data, 'r', encoding='utf-8') as f:
                    data = json.load(f)
            else:
                with open(data, 'rb') as f:
                    data = f.read()
        if not from_json:
            self.output_dict = {}
            self.max_blackboard_index = 0

            self.stream = ReadStream(data)
            self.functions = {}

            # Header (0x74 Bytes)
            self.magic = self.stream.read(4).decode('utf-8')
            if self.magic != "ASB ": # Must be .asb file with correct magic
                raise Exception(f"Invalid magic {self.magic} - expected 'ASB '")
            self.version = self.stream.read_u32()
            if self.version not in [0x417, 0x40F]: # Must be version 0x417 or 0x40F
                raise Exception(f"Invalid version {hex(self.version)} - expected 0x417 or 0x40F")
            
            self.filename_offset = self.stream.read_u32()
            self.command_count = self.stream.read_u32()
            self.node_count = self.stream.read_u32()
            self.event_count = self.stream.read_u32()
            self.slot_count = self.stream.read_u32()
            self.x38_count = self.stream.read_u32()
            self.local_blackboard_offset = self.stream.read_u32()
            self.string_pool_offset = self.stream.read_u32()
            
            # Create string pool slice
            jumpback = self.stream.tell()
            self.stream.seek(self.string_pool_offset)
            self.string_pool = ReadStream(self.stream.read())
            self.filename = self.string_pool.read_string(self.filename_offset)
            self.stream.seek(jumpback)

            self.enum_resolve_array_offset = self.stream.read_u32()
            self.x2c_offset = self.stream.read_u32()
            self.event_offsets_offset = self.stream.read_u32()
            self.slots_offset = self.stream.read_u32()
            self.x38_offset = self.stream.read_u32()
            self.x38_index_offset = self.stream.read_u32()
            self.x40_offset = self.stream.read_u32()
            self.x40_count = self.stream.read_u32()
            self.bone_group_offset = self.stream.read_u32()
            self.bone_group_count = self.stream.read_u32()
            self.string_pool_size = self.stream.read_u32()
            self.transitions_offset = self.stream.read_u32()
            self.tag_list_offset = self.stream.read_u32()
            self.as_markings_offset = self.stream.read_u32()
            self.exb_offset = self.stream.read_u32()
            self.command_groups_offset = self.stream.read_u32()
            if self.version == 0x417:
                self.x68_offset = self.stream.read_u32()

            self.output_dict["Info"] = {
                "Magic" : self.magic,
                "Version" : hex(self.version),
                "Filename" : self.filename
            }

            self.command_start = self.stream.tell()

            self.stream.seek(self.local_blackboard_offset)

            self.local_blackboard_params = self.LocalBlackboard()
            self.output_dict["Local Blackboard Parameters"] = self.local_blackboard_params

            if self.exb_offset:
                self.stream.seek(self.exb_offset)
                self.exb = EXB(self.stream.read())
                self.output_dict["EXB Section"] = self.exb.exb_section
            else:
                self.exb = {}

            self.stream.seek(self.x40_offset)
            self.x40_section = []
            for i in range(self.x40_count):
                self.x40_section.append(self.X40())

            self.stream.seek(self.command_start)
            if self.version == 0x417:
                assert self.stream.tell() == 0x6C, "Something went wrong, invalid header size"
            elif self.version == 0x40F:
                assert self.stream.tell() == 0x68, "Something went wrong, invalid header size"
            else:
                raise ValueError(f"Invalid version: {hex(self.version)}")
            self.commands = []
            for i in range(self.command_count):
                self.commands.append(self.Command())
            self.output_dict["Commands"] = self.commands
            self.node_start = self.stream.tell()

            self.stream.seek(self.event_offsets_offset)
            self.events = []
            for i in range(self.event_count):
                self.events.append(self.Event())

            self.stream.seek(self.x38_offset)
            self.x38_section = []
            for i in range(self.x38_count):
                self.x38_section.append(self.X38())

            self.stream.seek(self.bone_group_offset)
            self.bone_groups = []
            for i in range(self.bone_group_count):
                self.bone_groups.append(self.BoneGroup())

            self.command_groups = []
            if self.command_groups_offset:
                self.stream.seek(self.command_groups_offset)
                for i in range(self.stream.read_u32()):
                    self.command_groups.append(self.CommandGroup())

            self.stream.seek(self.transitions_offset)
            count = self.stream.read_u32()
            unknown = self.stream.read_u32() # usually 0? not sure if I should include it
            self.transitions = []
            for i in range(count):
                self.transitions.append(self.Transition())
            self.output_dict["Transitions"] = self.transitions

            self.stream.seek(self.tag_list_offset)
            count = self.stream.read_u32()
            self.tag_list = []
            for i in range(count):
                self.tag_list.append(self.string_pool.read_string(self.stream.read_u32()))
            self.output_dict["Valid Tag List"] = self.tag_list

            self.stream.seek(self.slots_offset)
            self.slots = []
            for i in range(self.slot_count):
                self.slots.append(self.Slot())
            self.output_dict["Animation Slots"] = self.slots

            self.stream.seek(self.as_markings_offset)
            count = self.stream.read_u32()
            self.as_markings = []
            for i in range(count):
                self.as_markings.append(self.ASMarking())

            if self.version == 0x417:
                self.stream.seek(self.x68_offset)
                count = self.stream.read_u32()
                self.x68_section = []
                for i in range(count):
                    self.x68_section.append(self.X68())
                self.output_dict["0x68 Section"] = self.x68_section

            # Unused in 0x417, only used in 0x40F (just like AINB)
            self.stream.seek(self.enum_resolve_array_offset)
            count = self.stream.read_u32()
            self.enum_resolve = {}
            for i in range(count):
                offset, value = self.EnumResolve()
                self.enum_resolve[offset] = value

            self.stream.seek(self.x2c_offset)
            count = self.stream.read_u32()
            self.x2c_section = []
            for i in range(count):
                self.x2c_section.append(self.X2C())

            self.stream.seek(self.node_start)
            self.nodes = {}
            for i in range(self.node_count):
                self.nodes[i] = self.Node()
            self.output_dict["Nodes"] = self.nodes
        else:
            self.output_dict = data
            self.version = int(data["Info"]["Version"], 16)
            if self.version not in [0x417, 0x40F]:
                raise ValueError(f"Invalid version {self.version}, expected 0x417 or 0x40F")
            self.filename = data["Info"]["Filename"]
            self.nodes = data["Nodes"]
            self.x68_section = data["0x68 Section"]
            self.slots = data["Animation Slots"]
            self.tag_list = data["Valid Tag List"]
            self.transitions = data["Transitions"]
            self.commands = data["Commands"]
            self.local_blackboard_params = data["Local Blackboard Parameters"]
            if "EXB Section" in data:
                self.exb = EXB(None, data["EXB Section"], from_dict=True)
            else:
                self.exb = {}
            self.x40_section = []
            self.x38_section = []
            self.events = []
            self.bone_groups = []
            self.command_groups = []
            self.as_markings = []
            if self.version == 0x417:
                self.x68_section = []
            self.enum_resolve = {} # not doing this bc I don't feel like it
            self.x2c_section = []
            for index in self.nodes:
                for entry in self.nodes[index]["0x40 Entries"]:
                    self.x40_section.append(entry)
                for entry in self.nodes[index]["0x38 Entries"]:
                    if entry not in self.x38_section:
                        self.x38_section.append(entry)
                if self.nodes[index]["Node Type"] == "Event":
                    self.events.append(self.nodes[index]["Body"]["Event"])
                if self.nodes[index]["Node Type"] == "BoneBlender":
                    if self.nodes[index]["Body"]["Bone Group"] not in self.bone_groups:
                        self.bone_groups.append(self.nodes[index]["Body"]["Bone Group"])
                if "ASMarkings" in self.nodes[index]:
                    if self.nodes[index]["ASMarkings"] not in self.as_markings:
                        self.as_markings.append(self.nodes[index]["ASMarkings"])
                if "Body" in self.nodes[index]:
                    if "0x2C Connections" in self.nodes[index]["Body"]:
                        for entry in self.nodes[index]["Body"]["0x2C Connections"]:
                            self.x2c_section.append(entry["0x2C Entry"])
            if self.version == 0x417:
                for transition in self.transitions:
                    for entry in transition["Transitions"]:
                        if "Command Group" in entry:
                            self.command_groups.append(entry["Command Group"])

    # monkas
    def ParseParameter(self, type):
        flags = self.stream.read_s32()
        # These flags are hurting my brain so here's a simplified version that works well enough until I figure them out
        if flags < 0:
            index = flags & 0xFFFF
            flag = (flags & 0xFFFF0000) >> 16
            if (flags ^ 0xFFFFFFFF) & 0x81000000 == 0:
                value = {"EXB Index" : index}
            elif type not in ["float", "vec3f"]:
                value = {"Flags" : hex(flag), "Type" : type, "Local Blackboard Index" : index}
            else:
                # fuck the other flags
                if (flag >> 0xe) < 3 or (flag >> 8) & 1:
                    if (flag >> 9) & 1 == 0:
                        value = {"Flags" : hex(flag), "Type" : type, "Local Blackboard Index" : index}
                    else:
                        value = {"Flags" : hex(flag), "Index" : index}
                else:
                    # usually means it's a 0x40 section index and the value is then calculated from other parameters
                    value = {"Flags" : hex(flag), "Index" : index}
            if type == "string":
                v = self.string_pool.read_string(self.stream.read_u32())
            elif type == "int":
                v = self.stream.read_s32()
            elif type == "float":
                v = self.stream.read_f32()
            elif type == "bool":
                v = bool(self.stream.read_u32())
            elif type == "vec3f":
                v = [self.stream.read_f32(), self.stream.read_f32(), self.stream.read_f32()]
            else:
                raise ValueError(f"Invalid parameter type: {type}")
            if v:
                value["Default Value"] = v
        else:
            if type == "string":
                value = self.string_pool.read_string(self.stream.read_u32())
            elif type == "int":
                value = self.stream.read_s32()
            elif type == "float":
                value = self.stream.read_f32()
            elif type == "bool":
                value = bool(self.stream.read_u32())
            elif type == "vec3f":
                value = [self.stream.read_f32(), self.stream.read_f32(), self.stream.read_f32()]
            else:
                raise ValueError(f"Invalid parameter type: {type}")
        return value
    
    def BlackboardHeader(self):
        entry = {}
        entry["Count"] = self.stream.read_u16()
        entry["Index"] = self.stream.read_u16()
        entry["Offset"] = self.stream.read_u16()
        self.stream.read_u16()
        return entry

    def BlackboardParam(self):
        entry = {}
        bitfield = self.stream.read_u32()
        valid_index = bool(bitfield >> 31)
        if valid_index:
            entry["Index"] = (bitfield >> 24) & 0b1111111
            if entry["Index"] > self.max_blackboard_index:
                self.max_blackboard_index = entry["Index"]
        name_offset = bitfield & 0x3FFFFF
        entry["Name"] = self.string_pool.read_string(name_offset)
        return entry
    
    def BlackboardParamValue(self, type):
        if type == "int":
            value = self.stream.read_u32()
        if type == "bool":
            value = bool(self.stream.read_u32())
        if type == "float":
            value = self.stream.read_f32()
        if type == "string":
            value = self.string_pool.read_string(self.stream.read_u32())
        if type == "vec3f":
            value = [self.stream.read_f32(), self.stream.read_f32(), self.stream.read_f32()]
        if type == "userdefined":
            value = None
        return value

    def FileRef(self):
        entry = {}
        entry["Filename"] = self.string_pool.read_string(self.stream.read_u32())
        entry["Filepath Hash"] = hex(self.stream.read_u32())
        entry["Filename Hash"] = hex(self.stream.read_u32())
        entry["File Extension Hash"] = hex(self.stream.read_u32())
        del entry["Filepath Hash"], entry["Filename Hash"], entry["File Extension Hash"]
        return entry

    def LocalBlackboard(self):
        self.blackboard_header = {}
        for type in type_param:
            self.blackboard_header[type] = self.BlackboardHeader()
        self.local_blackboard_params = {}
        for type in self.blackboard_header:
            parameters = []
            for i in range(self.blackboard_header[type]["Count"]):
                entry = self.BlackboardParam()
                parameters.append(entry)
            self.local_blackboard_params[type] = parameters
        pos = self.stream.tell()
        for type in self.local_blackboard_params:
            self.stream.seek(pos + self.blackboard_header[type]["Offset"])
            for entry in self.local_blackboard_params[type]:
                entry["Init Value"] = self.BlackboardParamValue(type)
        self.blackboard_refs = []
        for i in range(self.max_blackboard_index + 1):
            self.blackboard_refs.append(self.FileRef())
        for type in self.local_blackboard_params: # Match file references to parameters
            for entry in self.local_blackboard_params[type]:
                if "Index" in entry:
                    entry["File Reference"] = self.blackboard_refs[entry["Index"]]
                    del entry["Index"]
        self.local_blackboard_params = {key : value for key, value in self.local_blackboard_params.items() if value} # Remove types with no entries
        return self.local_blackboard_params
    
    def TagGroup(self):
        count = self.stream.read_u32()
        tags = []
        for i in range(count):
            tags.append(self.string_pool.read_string(self.stream.read_u32()))
        return tags
    
    def GUID(self) -> str:
        return hex(self.stream.read_u32())[2:] + "-" + hex(self.stream.read_u16())[2:] + "-" + hex(self.stream.read_u16())[2:] \
            + "-" + hex(self.stream.read_u16())[2:] + "-" + self.stream.read(6).hex()
    
    def Command(self):
        command = {}
        command["Name"] = self.string_pool.read_string(self.stream.read_u32())
        if self.version == 0x417:
            tag_offset = self.stream.read_u32()
            if tag_offset:
                pos = self.stream.tell()
                self.stream.seek(tag_offset)
                command["Tags"] = self.TagGroup()
                self.stream.seek(pos)
        # I have no idea if these three are correct
        command["Unknown 1"] = self.ParseParameter("float") # some type of keyframe
        command["Unknown 2"] = self.ParseParameter("int")
        command["Unknown 3"] = self.stream.read_u32()
        command["GUID"] = self.GUID()
        command["Left Node Index"] = self.stream.read_u16()
        command["Right Node Index"] = self.stream.read_u16() - 1
        return command
    
    def EventParameter(self):
        values = []
        offsets = []
        count = self.stream.read_u32()
        for i in range(count):
            offsets.append(self.stream.read_u32())
        for offset in offsets:
            flag = (offset & 0xFF000000) >> 24 # top byte is the data type
            offset = offset & 0xFFFFFF
            self.stream.seek(offset)
            if flag == 0x40:
                values.append(self.ParseParameter("string"))
            elif flag == 0x30:
                values.append(self.ParseParameter("float"))
            elif flag == 0x20:
                values.append(self.ParseParameter("int"))
            elif flag == 0x10:
                values.append(self.ParseParameter("bool"))
            else:
                raise ValueError(hex(flag), hex(offset)) # there might be a vec3f one but I haven't seen it before
        return values

    def TriggerEvent(self):
        event = {}
        event["Name"] = self.string_pool.read_string(self.stream.read_u32())
        event["Unknown 1"] = self.stream.read_u32()
        offset = self.stream.read_u32()
        param_size = self.stream.read_u32() # total size in bytes of all the parameter entries
        event["Unknown Hash"] = hex(self.stream.read_u32())
        event["Start Frame"] = self.stream.read_f32()
        pos = self.stream.tell()
        self.stream.seek(offset)
        event["Parameters"] = self.EventParameter()
        self.stream.seek(pos)
        return event
    
    def HoldEvent(self):
        event = {}
        event["Name"] = self.string_pool.read_string(self.stream.read_u32())
        event["Unknown 1"] = self.stream.read_u32()
        offset = self.stream.read_u32()
        param_size = self.stream.read_u32()
        event["Unknown Hash"] = hex(self.stream.read_u32())
        event["Start Frame"] = self.stream.read_f32()
        event["End Frame"] = self.stream.read_f32()
        pos = self.stream.tell()
        self.stream.seek(offset)
        event["Parameters"] = self.EventParameter()
        self.stream.seek(pos)
        return event

    def Event(self):
        offset = self.stream.read_u32()
        pos = self.stream.tell()
        self.stream.seek(offset)
        trigger_count = self.stream.read_u32()
        hold_count = self.stream.read_u32()
        event = {"Trigger Events" : [], "Hold Events" : []}
        for i in range(trigger_count):
            event["Trigger Events"].append(self.TriggerEvent())
        for i in range(hold_count):
            event["Hold Events"].append(self.HoldEvent())
        self.stream.seek(pos)
        return event

    # Honestly have no idea what this section is for
    def X38(self):
        entry = {}
        entry["Type"] = self.stream.read_u32()
        offset = self.stream.read_u32()
        entry["GUID"] = self.GUID()
        pos = self.stream.tell()
        self.stream.seek(offset)
        if entry["Type"] == 0:
            entry["Entry"] = {
                "Start Frame" : self.ParseParameter("float"),
                "Unknown 2" : self.stream.read_u32()
            }
        elif entry["Type"] == 1:
            entry["Entry"] = {
                "Start Frame" : self.ParseParameter("float"),
                "End Frame" : self.ParseParameter("float"),
                "Unknown 3" : self.ParseParameter("float")
            }
        elif entry["Type"] == 3:
            entry["Entry"] = {}
        else:
            raise ValueError(f"Invalid type {entry['Type']} - {hex(pos)}")
        self.stream.seek(pos)
        return entry
    
    # Seems to be some type of calculation preset?
    def X40(self):
        entry = {}
        # Don't take anything here as fact, these are complete guesses
        entry["Unknown 1"] = self.stream.read_u32()
        entry["Angle"] = self.stream.read_f32()
        if self.version == 0x417:
            entry["Type"] = self.stream.read_u32() # Except this is definitely a type
        entry["Unknown 2"] = self.stream.read_f32()
        entry["Rate"] = self.stream.read_f32()
        entry["Unknown 3"] = self.stream.read_f32() # Base value for calculations
        entry["Min"] = self.stream.read_f32() # Minimum value or something?
        entry["Max"] = self.stream.read_f32() # Maximum value or something?
        return entry

    # Bone groups for animation blending
    def BoneGroup(self):
        entry = {}
        offset = self.stream.read_u32()
        entry["Name"] = self.string_pool.read_string(self.stream.read_u32())
        count = self.stream.read_u32()
        entry["Unknown"] = self.stream.read_u32()
        entry["Bones"] = []
        pos = self.stream.tell()
        self.stream.seek(offset)
        for i in range(count):
            bone = {}
            bone["Name"] = self.string_pool.read_string(self.stream.read_u32())
            bone["Unknown"] = self.stream.read_f32() # weight or something? idk anything about animation blending though
            entry["Bones"].append(bone)
        self.stream.seek(pos)
        return entry

    # Command groups for transitions
    def CommandGroup(self):
        entry = []
        offset = self.stream.read_u32()
        pos = self.stream.tell()
        self.stream.seek(offset)
        count = self.stream.read_u32()
        for i in range(count):
            entry.append(self.string_pool.read_string(self.stream.read_u32()))
        self.stream.seek(pos)
        return entry

    # idk if transition is the best term here, but these do update their specified parameter at the corresponding cmd transition
    def TransitionEntry(self):
        entry = {}
        types = {
            0 : "int",
            1 : "string",
            2 : "float",
            3 : "bool",
            4 : "vec3f"
        }
        entry["Command 1"] = self.string_pool.read_string(self.stream.read_u32())
        entry["Command 2"] = self.string_pool.read_string(self.stream.read_u32())
        enum = self.stream.read_u8()
        if enum in types:
            entry["Parameter Type"] = types[enum]
        else:
            entry["Parameter Type"] = "vec3f"
        entry["Allow Multiple Matches"] = bool(self.stream.read_u8())
        cmd_group_index = self.stream.read_u16() - 1
        entry["Parameter"] = self.string_pool.read_string(self.stream.read_u32())
        entry["Value"] = self.ParseParameter(entry["Parameter Type"])
        if entry["Parameter Type"] != "vec3f":
            self.stream.read(8)
        if cmd_group_index >= 0:
            entry["Command Group"] = self.command_groups[cmd_group_index]
        return entry

    def Transition(self):
        entry = {}
        count = self.stream.read_u32()
        entry["Unknown"] = self.stream.read_s32()
        offset = self.stream.read_u32()
        pos = self.stream.tell()
        self.stream.seek(offset)
        entry["Transitions"] = []
        for i in range(count):
            entry["Transitions"].append(self.TransitionEntry())
        self.stream.seek(pos)
        return entry

    def ASMarking(self):
        return [self.string_pool.read_string(self.stream.read_u32()), # "ASMarking"
                 self.string_pool.read_string(self.stream.read_u32()), # "ASマーキング" (same thing but in jp)
                 self.string_pool.read_string(self.stream.read_u32())] # marking name

    def X68(self):
        entry = {}
        entry["Name"] = self.string_pool.read_string(self.stream.read_u32())
        entry["Unknown"] = self.stream.read_f32()
        return entry
    
    # Only used in version 0x40F even though it's present in 0x417
    # 0x417 enum values are listed in the string pool but are never used
    def EnumResolve(self):
        entry = {}
        offset = self.stream.read_u32()
        entry["Class Name"] = self.string_pool.read_string(self.stream.read_u32())
        entry["Value Name"] = self.string_pool.read_string(self.stream.read_u32())
        return offset, entry
    
    def X2CSubEntry(self):
        entry = {}
        entry["Entry Type"] = self.stream.read_u16()
        entry["Unknown Type"] = self.stream.read_u16() # determines the comparison type later but idk what it's for
        if entry["Entry Type"] == 0:
            self.stream.read(16) # No data
        elif entry["Entry Type"] == 1:
            entry["Unknown 1"] = self.ParseParameter("float")
            entry["Unknown 2"] = self.ParseParameter("float")
        elif entry["Entry Type"] == 2:
            entry["Unknown 1"] = self.ParseParameter("int")
            entry["Unknown 2"] = self.ParseParameter("int")
        elif entry["Entry Type"] == 3:
            entry["Unknown 1"] = self.ParseParameter("bool")
            entry["Unknown 2"] = self.ParseParameter("bool")
        else:
            entry["Unknown 1"] = self.ParseParameter("string")
            entry["Unknown 2"] = self.ParseParameter("string")
        return entry

    # Might be used as a way for two nodes to share certain values with each other?
    def X2C(self):
        entry = {}
        entry["Source Node"] = self.stream.read_u16()
        entry["Target Node"] = self.stream.read_u16()
        entry["Unknown 1"] = self.stream.read_u32()
        entry["Unknown 2"] = self.stream.read_u32()
        entry["Unknown 3"] = self.stream.read_u32()
        entry["Entries"] = [
            self.X2CSubEntry(), self.X2CSubEntry(), self.X2CSubEntry(), self.X2CSubEntry()
        ]
        return entry
    
    def Slot(self):
        entry = {}
        count = self.stream.read_u16()
        entry["Unknown"] = self.stream.read_u16()
        entry["Partial 1"] = self.string_pool.read_string(self.stream.read_u32())
        entry["Partial 2"] = self.string_pool.read_string(self.stream.read_u32())
        entry["Entries"] = []
        for i in range(count):
            slot = {}
            slot["Bone"] = self.string_pool.read_string(self.stream.read_u32())
            slot["Unknown 1"] = self.stream.read_u16()
            slot["Unknown 2"] = self.stream.read_u16()
            entry["Entries"].append(slot)
        return entry
    
    def Node(self):
        node = {}
        node["Node Type"] = NodeType(self.stream.read_u16()).name
        x3c_count = self.stream.read_u8()
        node["Unknown"] = self.stream.read_u8()
        tag_offset = self.stream.read_u32()
        if tag_offset:
            pos = self.stream.tell()
            self.stream.seek(tag_offset)
            node["Tags"] = self.TagGroup()
            self.stream.seek(pos)
        body_offset = self.stream.read_u32()
        x40_index = self.stream.read_u16()
        x40_count = self.stream.read_u16()
        x3c_index = self.stream.read_u16()
        as_markings_index = self.stream.read_u16() - 1
        node["GUID"] = self.GUID()
        pos = self.stream.tell()
        node["0x38 Entries"] = []
        node["0x40 Entries"] = self.x40_section[x40_index:x40_index+x40_count]
        if as_markings_index >= 0:
            node["ASMarkings"] = self.as_markings[as_markings_index]
        if x3c_count:
            self.stream.seek(self.x38_index_offset + 4 * x3c_index)
            for i in range(x3c_count):
                node["0x38 Entries"].append(self.x38_section[self.stream.read_u32()])
        self.stream.seek(body_offset)
        if node["Node Type"] == "FloatSelector":
            node["Body"] = self.FloatSelector()
        elif node["Node Type"] == "StringSelector":
            node["Body"] = self.StringSelector()
        elif node["Node Type"] == "SkeletalAnimation":
            node["Body"] = self.SkeletalAnimation()
        elif node["Node Type"] == "State":
            node["Body"] = self.State()
        elif node["Node Type"] == "Unknown2":
            pass # No node body
        elif node["Node Type"] == "OneDimensionalBlender":
            node["Body"] = self.OneDimensionalBlender()
        elif node["Node Type"] == "Sequential":
            node["Body"] = self.Sequential()
        elif node["Node Type"] == "IntSelector":
            node["Body"] = self.IntSelector()
        elif node["Node Type"] == "Simultaneous":
            node["Body"] = self.Simultaneous()
        elif node["Node Type"] == "Event":
            node["Body"] = self.EventNode()
        elif node["Node Type"] == "MaterialAnimation":
            node["Body"] = self.MaterialAnimation()
        elif node["Node Type"] == "FrameController":
            node["Body"] = self.FrameController()
        elif node["Node Type"] == "DummyAnimation":
            node["Body"] = self.DummyAnimation()
        elif node["Node Type"] == "RandomSelector":
            node["Body"] = self.RandomSelector()
        elif node["Node Type"] == "Unknown4":
            pass # No node body
        elif node["Node Type"] == "PreviousTagSelector":
            node["Body"] = self.PreviousTagSelector()
        elif node["Node Type"] == "BonePositionSelector":
            node["Body"] = self.BonePositionSelector()
        elif node["Node Type"] == "BoneAnimation":
            node["Body"] = self.BoneAnimation()
        elif node["Node Type"] == "InitialFrame":
            node["Body"] = self.InitialFrame()
        elif node["Node Type"] == "BoneBlender":
            node["Body"] = self.BoneBlender()
        elif node["Node Type"] == "BoolSelector":
            node["Body"] = self.BoolSelector()
        elif node["Node Type"] == "Alert":
            node["Body"] = self.Alert()
        elif node["Node Type"] == "SubtractAnimation":
            node["Body"] = self.SubtractAnimation()
        elif node["Node Type"] == "ShapeAnimation":
            node["Body"] = self.ShapeAnimation()
        elif node["Node Type"] == "Unknown7":
            node["Body"] = self.Unknown7()
        self.stream.seek(pos)
        return node
    
    def NodeConnections(self):
        offsets = {"State" : [], "Unk" : [], "Child" : [], "0x2c" : [], "Event" : [], "Frame" : []}
        # This type is used by State nodes but I haven't seen any these nodes used ever
        state_count = self.stream.read_u8()
        state_index = self.stream.read_u8() # base index, same goes for the other types
        # Appear to be unused as far as I can tell
        unknown_count = self.stream.read_u8()
        unknown_index = self.stream.read_u8()
        child_count = self.stream.read_u8()
        child_index = self.stream.read_u8()
        x2c_count = self.stream.read_u8()
        x2c_index = self.stream.read_u8()
        # Event nodes not directly events
        event_count = self.stream.read_u8()
        event_index = self.stream.read_u8()
        # FrameController or InitialFrame nodes
        frame_count = self.stream.read_u8()
        frame_index = self.stream.read_u8()
        for i in range(state_count):
            offsets["State"].append(self.stream.read_u32())
        for i in range(unknown_count):
            offsets["Unk"].append(self.stream.read_u32())
        for i in range(child_count):
            offsets["Child"].append(self.stream.read_u32())
        for i in range(x2c_count):
            offsets["0x2c"].append(self.stream.read_u32())
        for i in range(event_count):
            offsets["Event"].append(self.stream.read_u32())
        for i in range(frame_count):
            offsets["Frame"].append(self.stream.read_u32())
        state = []
        if offsets["State"]:
            for offset in offsets["State"]:
                self.stream.seek(offset)
                state.append(self.stream.read_u32())
        x2c = []
        if offsets["0x2c"]:
            for offset in offsets["0x2c"]:
                self.stream.seek(offset)
                if self.version == 0x417:
                    index = self.stream.read_s32()
                    entry = {"0x2C Entry" : {}, "Node Index" : -1}
                    if index >= 0:
                        entry["0x2C Entry"] = self.x2c_section[index]
                    entry["Node Index"] = self.stream.read_u32()
                    x2c.append(entry)
                else:
                    x2c.append(self.stream.read_u32()) # 0x40F only has the node index
        event = []
        if offsets["Event"]:
            for offset in offsets["Event"]:
                self.stream.seek(offset)
                event.append(self.stream.read_u32())
        frame = []
        if offsets["Frame"]:
            for offset in offsets["Frame"]:
                self.stream.seek(offset)
                frame.append(self.stream.read_u32())
        return offsets, x2c, event, frame, state

    def FloatSelector(self):
        entry = {}
        entry["Parameter"] = self.ParseParameter("float")
        entry["Unknown 1"] = self.ParseParameter("bool")
        entry["Unknown 2"] = bool(self.stream.read_u32())
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                child = {}
                if len(entry["Child Nodes"]) == len(offsets["Child"]) - 1:
                    child["Default Condition"] = self.ParseParameter("string")
                    self.stream.read(8) # empty
                    child["Node Index"] = self.stream.read_u32() 
                else:
                    child["Condition Min"] = self.ParseParameter("float")
                    child["Condition Max"] = self.ParseParameter("float")
                    child["Node Index"] = self.stream.read_u32()
                entry["Child Nodes"].append(child)
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def StringSelector(self):
        entry = {}
        entry["Parameter"] = self.ParseParameter("string")
        entry["Unknown 1"] = self.ParseParameter("bool")
        entry["Unknown 2"] = bool(self.stream.read_u32())
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                child = {}
                if len(entry["Child Nodes"]) == len(offsets["Child"]) - 1:
                    child["Default Condition"] = self.ParseParameter("string")
                    child["Node Index"] = self.stream.read_u32() 
                else:
                    child["Condition"] = self.ParseParameter("string")
                    child["Node Index"] = self.stream.read_u32()
                entry["Child Nodes"].append(child)
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def SkeletalAnimation(self):
        entry = {}
        entry["Animation"] = self.ParseParameter("string")
        entry["Unknown 1"] = self.stream.read_u32()
        entry["Unknown 2"] = self.stream.read_u32()
        entry["Unknown 3"] = self.ParseParameter("bool")
        entry["Unknown 4"] = self.ParseParameter("float")
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def State(self):
        entry = {}
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def Unknown2(self):
        return {} # No node body

    def OneDimensionalBlender(self):
        entry = {}
        entry["Parameter"] = self.ParseParameter("float")
        entry["Unknown"] = self.stream.read_u32()
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                child = {}
                child["Condition Min"] = self.ParseParameter("float")
                child["Condition Max"] = self.ParseParameter("float")
                child["Node Index"] = self.stream.read_u32()
                entry["Child Nodes"].append(child)
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def Sequential(self):
        entry = {}
        entry["Unknown 1"] = self.ParseParameter("bool")
        entry["Unknown 2"] = self.ParseParameter("int")
        entry["Unknown 3"] = self.ParseParameter("int") # Unsure of data type
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def IntSelector(self):
        entry = {}
        entry["Parameter"] = self.ParseParameter("int")
        entry["Unknown 1"] = self.ParseParameter("bool")
        entry["Unknown 2"] = bool(self.stream.read_u32())
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                child = {}
                if len(entry["Child Nodes"]) == len(offsets["Child"]) - 1:
                    child["Default Condition"] = self.ParseParameter("int")
                    child["Node Index"] = self.stream.read_u32() 
                else:
                    child["Condition"] = self.ParseParameter("int")
                    if child["Condition"] == 0:
                        if (self.stream.tell() - 4) in self.enum_resolve:
                            child["Condition"] = self.enum_resolve[self.stream.tell() - 4]
                    child["Node Index"] = self.stream.read_u32()
                entry["Child Nodes"].append(child)
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def Simultaneous(self):
        entry = {}
        entry["Unknown"] = self.stream.read_u32()
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def EventNode(self):
        entry = {}
        index = self.stream.read_u32()
        entry["Event"] = self.events[index] # literally a scam
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def MaterialAnimation(self):
        entry = {}
        if self.version == 0x417:
            entry["Unknown 1"] = self.stream.read_u32()
        entry["Animation"] = self.ParseParameter("string")
        entry["Unknown 2"] = self.ParseParameter("bool")
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def FrameController(self):
        entry = {}
        entry["Animation Rate"] = self.ParseParameter("float") # 0x00
        entry["Start Frame"] = self.ParseParameter("float") # 0x08
        entry["End Frame"] = self.ParseParameter("float") # 0x10
        entry["Unknown Flag"] = self.stream.read_u32() # 0x18
        entry["Loop Cancel Flag"] = self.ParseParameter("bool") # 0x1c
        entry["Unknown 2"] = self.ParseParameter("bool") # 0x24
        entry["Unknown 3"] = self.ParseParameter("int") # 0x2c
        entry["Unknown 4"] = self.ParseParameter("int") # 0x34
        entry["Unknown 5"] = self.ParseParameter("bool") # 0x3c
        entry["Unknown 6"] = self.ParseParameter("float") # 0x44
        entry["Unknown 7"] = self.ParseParameter("float") # 0x4c
        entry["Unknown 8"] = self.ParseParameter("float") # 0x58
        entry["Unknown 9"] = bool(self.stream.read_u32()) # 0x5c
        entry["Unknown 10"] = self.ParseParameter("float") # 0x60
        if self.version == 0x417: # Unsure if this is the specific one that's missing
            entry["Unknown 11"] = self.ParseParameter("bool") # 0x68
        entry["Unknown 12"] = self.stream.read_u32() # 0x70
        entry["Unknown 13"] = self.stream.read_u32() # 0x74
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry
    
    def DummyAnimation(self):
        entry = {}
        entry["Frame"] = self.ParseParameter("float")
        entry["Unknown"] = self.ParseParameter("bool")
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def RandomSelector(self):
        entry = {}
        entry["Unknown 1"] = self.stream.read_u32()
        entry["Unknown 2"] = self.ParseParameter("bool")
        entry["Unknown 3"] = self.ParseParameter("int")
        entry["Unknown 4"] = bool(self.stream.read_u32())
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                child = {}
                child["Weight"] = self.ParseParameter("float")
                child["Node Index"] = self.stream.read_u32()
                entry["Child Nodes"].append(child)
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def Unknown4(self):
        return {} # No node body

    def PreviousTagSelector(self):
        entry = {}
        entry["Unknown"] = self.stream.read_u32()
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                link = {"Tags" : [], "Node Index" : -1}
                self.stream.seek(offset)
                tag_offset = self.stream.read_u32()
                link["Node Index"] = self.stream.read_u32()
                if tag_offset != 0xFFFFFFFF:
                    self.stream.seek(tag_offset)
                    link["Tags"] = self.TagGroup()
                entry["Child Nodes"].append(link)
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def BonePositionSelector(self):
        entry = {}
        entry["Bone 1"] = self.ParseParameter("string")
        entry["Bone 2"] = self.ParseParameter("string")
        entry["Unknown 1"] = self.stream.read_u32()
        entry["Unknown 2"] = self.stream.read_u32()
        entry["Unknown 3"] = self.ParseParameter("bool")
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                child = {}
                if len(entry["Child Nodes"]) == len(offsets["Child"]) - 1:
                    child["Default Condition"] = self.ParseParameter("string")
                    self.stream.read(8) # empty
                    child["Node Index"] = self.stream.read_u32() 
                else:
                    child["Condition Min"] = self.ParseParameter("float")
                    child["Condition Max"] = self.ParseParameter("float")
                    child["Node Index"] = self.stream.read_u32()
                entry["Child Nodes"].append(child)
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def BoneAnimation(self):
        entry = {}
        entry["Animation"] = self.ParseParameter("string")
        entry["Unknown 1"] = self.ParseParameter("bool")
        entry["Unknown 2"] = self.stream.read_u32()
        entry["Unknown 3"] = self.stream.read_u32()
        entry["Unknown 4"] = self.ParseParameter("float")
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def InitialFrame(self):
        entry = {}
        entry["Flag"] = self.stream.read_u32()
        tag_offset = self.stream.read_u32()
        if tag_offset:
            pos = self.stream.tell()
            self.stream.seek(tag_offset)
            entry["Tags"] = self.TagGroup()
            self.stream.seek(pos)
        if self.version == 0x417:
            entry["Unknown 1"] = self.ParseParameter("bool")
        entry["Bone 1"] = self.ParseParameter("string") # Used if flag is 4
        entry["Bone 2"] = self.ParseParameter("string") # Used if flag is 4
        entry["Unknown 2"] = self.stream.read_u32() # Involved in some comparison of the two bones
        entry["Unknown 3"] = self.ParseParameter("bool")
        entry["Unknown 4"] = self.ParseParameter("bool")
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def BoneBlender(self):
        entry = {}
        name = self.ParseParameter("string")
        for group in self.bone_groups:
            if group["Name"] == name:
                entry["Bone Group"] = group
        entry["Unknown 1"] = self.stream.read_u32()
        entry["Unknown 2"] = self.ParseParameter("float")
        entry["Unknown 3"] = self.stream.read_u32()
        if self.version == 0x417:
            entry["Unknown 4"] = self.stream.read_u32()
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def BoolSelector(self):
        entry = {}
        entry["Parameter"] = self.ParseParameter("bool")
        entry["Unknown 1"] = self.ParseParameter("bool")
        entry["Unknown 2"] = bool(self.stream.read_u32())
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                if offsets["Child"].index(offset) == 0:
                    entry["Child Nodes"].append({"Condition True" : self.stream.read_u32()})
                else:
                    entry["Child Nodes"].append({"Condition False" : self.stream.read_u32()})
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    # I think this is for debugging purposes, I don't think these nodes are ever reached despite being present
    def Alert(self):
        entry = {}
        entry["Message"] = self.ParseParameter("string")
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def SubtractAnimation(self):
        entry = {}
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def ShapeAnimation(self):
        entry = {}
        entry["Animation"] = self.ParseParameter("string")
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry

    def Unknown7(self):
        entry = {}
        offsets, x2c, event, frame, state = self.NodeConnections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Nodes"] = state
        if x2c:
            entry["0x2C Connections"] = x2c
        if event:
            entry["Event Node Connections"] = event
        if frame:
            entry["Frame Node Connections"] = frame
        return entry
    
    @staticmethod
    def CalcBodySize(node, version):
        if node["Node Type"] == "FloatSelector":
            size = 0x20
        elif node["Node Type"] == "StringSelector":
            size = 0x20
        elif node["Node Type"] == "SkeletalAnimation":
            size = 0x2c
        elif node["Node Type"] == "State":
            size = 0xc
        elif node["Node Type"] == "Unknown2":
            size = 0x0
        elif node["Node Type"] == "OneDimensionalBlender":
            size = 0x18
        elif node["Node Type"] == "Sequential":
            size = 0x24
        elif node["Node Type"] == "IntSelector":
            size = 0x20
        elif node["Node Type"] == "Simultaneous":
            size = 0x10
        elif node["Node Type"] == "Event":
            size = 0x10
        elif node["Node Type"] == "MaterialAnimation":
            size = 0x20
        elif node["Node Type"] == "FrameController":
            size = 0x84 if version == 0x417 else 0x7c
        elif node["Node Type"] == "DummyAnimation":
            size = 0x1c
        elif node["Node Type"] == "RandomSelector":
            size = 0x24
        elif node["Node Type"] == "Unknown4":
            size = 0x0
        elif node["Node Type"] == "PreviousTagSelector":
            size = 0x10
        elif node["Node Type"] == "BonePositionSelector":
            size = 0x2c
        elif node["Node Type"] == "BoneAnimation":
            size = 0x2c
        elif node["Node Type"] == "InitialFrame":
            size = 0x40
        elif node["Node Type"] == "BoneBlender":
            size = 0x28
        elif node["Node Type"] == "BoolSelector":
            size = 0x20
        elif node["Node Type"] == "Alert":
            size = 0x14
        elif node["Node Type"] == "SubtractAnimation":
            size = 0xc
        elif node["Node Type"] == "ShapeAnimation":
            size = 0x14
        elif node["Node Type"] == "Unknown7":
            size = 0xc
        if "Body" in node:
            if "State Nodes" in node["Body"]:
                size += 8 * len(node["Body"]["State Nodes"]) # 4 for the offset and 4 for the index
            if "0x2C Connections" in node["Body"]:
                if version == 0x40F:
                    size += 8 * len(node["Body"]["0x2C Connections"]) # 4 for the offset and 4 for the index
                else:
                    size += 12 * len(node["Body"]["0x2C Connections"]) # 4 for the offset and 8 for the two indices
            if "Event Node Connections" in node["Body"]:
                size += 8 * len(node["Body"]["Event Node Connections"]) # 4 for the offset and 4 for the index
            if "Frame Node Connections" in node["Body"]:
                size += 8 * len(node["Body"]["Frame Node Connections"]) # 4 for the offset and 4 for the index
            if "Child Nodes" in node["Body"]:
                if node["Node Type"] in ["BonePositionSelector", "FloatSelector", "OneDimensionalBlender"]:
                    size += 24 * len(node["Body"]["Child Nodes"]) # 4 for the offset and 16 for the two conditions and 4 for the index
                elif node["Node Type"] in ["RandomSelector", "IntSelector", "StringSelector"]:
                    size += 16 * len(node["Body"]["Child Nodes"]) # 4 for the offset and 8 for the condition and 4 for the index
                elif node["Node Type"] == "PreviousTagSelector":
                    size += 12 * len(node["Body"]["Child Nodes"]) # 4 for the offset, 4 for the tag offset, 4 for the index
                else:
                    size += 8 * len(node["Body"]["Child Nodes"]) # 4 for the offset and 4 for the index
        return size

    def WriteParameter(self, buffer, value):
        if type(value) != dict:
            buffer.write(u32(0))
            if type(value) == int:
                buffer.write(s32(value))
            elif type(value) == float:
                buffer.write(f32(value))
            elif type(value) == str:
                buffer.add_string(value)
                buffer.write(u32(buffer._string_refs[value]))
            elif type(value) == bool:
                buffer.write(u32(1 if value else 0))
            elif type(value) == list:
                for v in value:
                    buffer.write(f32(v))
            else:
                raise ValueError(f"Invalid value {value}")
        else:
            if "Flags" in value:
                flag = int(value["Flags"], 16) << 16
            else:
                flag = 0x81000000
            if "Index" in value:
                flag = flag | value["Index"] & 0xFFFF
            elif "Local Blackboard Index" in value:
                flag = flag | value["Local Blackboard Index"] & 0xFFFF
            elif "EXB Index" in value:
                flag = flag | value["EXB Index"] & 0xFFFF
            else:
                raise ValueError("Missing index")
            buffer.write(u32(flag))
            if "Default Value" in value:
                if type(value["Default Value"]) == int:
                    buffer.write(s32(value["Default Value"]))
                elif type(value["Default Value"]) == float:
                    buffer.write(f32(value["Default Value"]))
                elif type(value["Default Value"]) == str:
                    buffer.add_string(value["Default Value"])
                    buffer.write(u32(buffer._string_refs[value["Default Value"]]))
                elif type(value["Default Value"]) == bool:
                    buffer.write(u32(1 if value["Default Value"] else 0))
                elif type(value["Default Value"]) == list:
                    for v in value["Default Value"]:
                        buffer.write(f32(v))
            else:
                buffer.write(u32(0))

    # Let's just do this all now so we don't have to jump back and fill in the offsets later
    def CalcOffsets(self, body_sizes, event_count, x38, tag_groups, buffer):
        offsets = {}
        offset = 0x6C if self.version == 0x417 else 0x68
        offset += (0x30 if self.version == 0x417 else 0x2C) * len(self.commands)
        offset += 0x24 * len(self.nodes)
        offsets["Event Offsets"] = offset
        offset += 0x4 * event_count
        offsets["Node Bodies"] = offset
        for i in body_sizes:
            offset += body_sizes[i]
        offsets["0x38 Indices"] = offset
        offset += 0x4 * x38
        offsets["0x38"] = offset
        for entry in self.x38_section:
            offset += 0x18
            if entry["Type"] == 0:
                offset += 0xC
            elif entry["Type"] == 1:
                offset += 0x18
        offsets["0x2C"] = offset
        offset += 0x4 + 0x60 * len(self.x2c_section)
        offsets["Events"] = offset
        event_offsets = []
        for i, entry in enumerate(self.events):
            event_offsets.append(offset)
            offset += 0x8
            for event in entry["Trigger Events"]:
                offset += 0x18
                offset += 0x4 + 0xc * len(event["Parameters"])
            for event in entry["Hold Events"]:
                offset += 0x1C
                offset += 0x4 + 0xc * len(event["Parameters"])
        offsets["Transitions"] = offset
        offset += 0x8 + 0xc * len(self.transitions)
        for entry in self.transitions:
            offset += 0x20 * len(entry["Transitions"])
        if self.command_groups:
            offsets["Command Groups"] = offset
            offset += 0x4 + 0x8 * len(self.command_groups)
            for group in self.command_groups:
                offset += 0x4 * len(group)
        else:
            offsets["Command Groups"] = 0
        offsets["Local Blackboard"] = offset
        offset += 0x30
        refs = []
        for datatype in self.local_blackboard_params:
            offset += 0x4 * len(self.local_blackboard_params[datatype])
            if datatype in ["int", "float", "bool", "string"]:
                offset += 0x4 * len(self.local_blackboard_params[datatype])
            elif datatype == "vec3f":
                offset += 0xc * len(self.local_blackboard_params[datatype])
            for param in self.local_blackboard_params[datatype]:
                if "File Reference" in param:
                    if param["File Reference"] not in refs:
                        refs.append(param["File Reference"])
        offset += 0x10 * len(refs)
        offsets["Slots"] = offset
        for entry in self.slots:
            offset += 0xc + 0x8 * len(entry["Entries"])
        offsets["Bone Groups"] = offset
        for entry in self.bone_groups:
            offset += 0x10 + 0x8 * len(entry["Bones"])
        offsets["0x40"] = offset
        offset += 0x20 * len(self.x40_section)
        offsets["Tag List"] = offset
        offset += 4 + 4 * len(self.tag_list)
        offsets["Tag Groups"] = offset
        tag_map = {}
        for entry in tag_groups:
            tag_map[tuple(entry)] = offset
            offset += 4 + 4 * len(entry)
        if self.exb:
            offsets["EXB"] = offset
            pos = buffer.tell()
            offset = self.exb.ToBytes(self.exb, buffer, offsets["EXB"])
            buffer.seek(pos)
        else:
            offsets["EXB"] = 0
        offsets["ASMarkings"] = offset
        offset += 4 + 12 * len(self.as_markings)
        if self.version == 0x417:
            offsets["0x68"] = offset
            offset += 4 + 8 * len(self.x68_section)
        offsets["Enum"] = offset
        offset += 4 + 12 * len(self.enum_resolve)
        offsets["Strings"] = offset
        return offsets, tag_map, event_offsets

    def WriteConnections(self, buffer, node_body, node_type, tag_map={}):
        index = 0
        if "State Nodes" in node_body:
            buffer.write(u8(len(node_body["State Nodes"])))
            buffer.write(u8(index))
            index += len(node_body["State Nodes"])
        else:
            buffer.write(u8(0))
            buffer.write(u8(index))
        if "Unknown Connection" in node_body:
            buffer.write(u8(len(node_body["Unknown Connection"])))
            buffer.write(u8(index))
            index += len(node_body["Unknown Connection"])
        else:
            buffer.write(u8(0))
            buffer.write(u8(index))
        if "Child Nodes" in node_body:
            buffer.write(u8(len(node_body["Child Nodes"])))
            buffer.write(u8(index))
            index += len(node_body["Child Nodes"])
        else:
            buffer.write(u8(0))
            buffer.write(u8(index))
        if "0x2C Connections" in node_body:
            buffer.write(u8(len(node_body["0x2C Connections"])))
            buffer.write(u8(index))
            index += len(node_body["0x2C Connections"])
        else:
            buffer.write(u8(0))
            buffer.write(u8(index))
        if "Event Node Connections" in node_body:
            buffer.write(u8(len(node_body["Event Node Connections"])))
            buffer.write(u8(index))
            index += len(node_body["Event Node Connections"])
        else:
            buffer.write(u8(0))
            buffer.write(u8(index))
        if "Frame Node Connections" in node_body:
            buffer.write(u8(len(node_body["Frame Node Connections"])))
            buffer.write(u8(index))
            index += len(node_body["Frame Node Connections"])
        else:
            buffer.write(u8(0))
            buffer.write(u8(index))
        offset = buffer.tell() + 4 * index
        if "State Nodes" in node_body:
            for entry in node_body["State Nodes"]:
                buffer.write(u32(offset))
                offset += 4
        if "Child Nodes" in node_body:
            for entry in node_body["Child Nodes"]:
                buffer.write(u32(offset))
                if node_type in ["BonePositionSelector", "FloatSelector", "OneDimensionalBlender"]:
                    offset += 20
                elif node_type in ["RandomSelector", "IntSelector", "StringSelector"]:
                    offset += 12
                elif node_type == "PreviousTagSelector":
                    offset += 8
                else:
                    offset += 4
        if "0x2C Connections" in node_body:
            for entry in node_body["0x2C Connections"]:
                buffer.write(u32(offset))
                if self.version == 0x417:
                    offset += 8
                else:
                    offset += 4
        if "Event Node Connections" in node_body:
            for entry in node_body["Event Node Connections"]:
                buffer.write(u32(offset))
                offset += 4
        if "Frame Node Connections" in node_body:
            for entry in node_body["Frame Node Connections"]:
                buffer.write(u32(offset))
                offset += 4
        if "State Nodes" in node_body:
            for entry in node_body["State Nodes"]:
                buffer.write(u32(entry))
        if "Child Nodes" in node_body:
            for entry in node_body["Child Nodes"]:
                if node_type in ["BonePositionSelector", "FloatSelector", "OneDimensionalBlender"]:
                    if "Default Condition" in entry:
                        self.WriteParameter(buffer, entry["Default Condition"])
                        buffer.write(u64(0))
                        buffer.write(u32(entry["Node Index"]))
                    else:
                        self.WriteParameter(buffer, entry["Condition Min"])
                        self.WriteParameter(buffer, entry["Condition Max"])
                        buffer.write(u32(entry["Node Index"]))
                elif node_type in ["RandomSelector", "IntSelector", "StringSelector"]:
                    # Note to self to handle enum resolultion for 0x40F
                    if "Weight" in entry:
                        self.WriteParameter(buffer, entry["Weight"])
                    elif "Condition" in entry:
                        self.WriteParameter(buffer, entry["Condition"])
                    elif "Default Condition" in entry:
                        self.WriteParameter(buffer, entry["Default Condition"])
                    buffer.write(u32(entry["Node Index"]))
                elif node_type == "PreviousTagSelector":
                    if entry["Tags"]:
                        buffer.write(u32(tag_map[tuple(entry["Tags"])]))
                    else:
                        buffer.write(s32(-1))
                    buffer.write(u32(entry["Node Index"]))
                elif node_type == "BoolSelector":
                    if "Condition True" in entry:
                        buffer.write(u32(entry["Condition True"]))
                    elif "Condition False" in entry:
                        buffer.write(u32(entry["Condition False"]))
                else:
                    buffer.write(u32(entry))
        if "0x2C Connections" in node_body:
            for entry in node_body["0x2C Connections"]:
                if self.version == 0x417:
                    if entry["0x2C Entry"]:
                        buffer.write(u32(self.x2c_section.index(entry["0x2C Entry"])))
                    else:
                        buffer.write(s32(-1))
                    buffer.write(u32(entry["Node Index"]))
                else:
                    buffer.write(u32(entry))
        if "Event Node Connections" in node_body:
            for entry in node_body["Event Node Connections"]:
                buffer.write(u32(entry))
        if "Frame Node Connections" in node_body:
            for entry in node_body["Frame Node Connections"]:
                buffer.write(u32(entry))

    def ToBytes(self, cursor):
    # def ToBytes(self, output_dir=''):
    #     if output_dir:
    #         os.makedirs(output_dir, exist_ok=True)
    #     with open(os.path.join(output_dir, self.filename + ".asb"), 'wb') as f:
        # with cursor as f:
        buffer = WriteStream(cursor)
        buffer.write("ASB ".encode())
        buffer.write(u32(self.version))
        buffer.add_string(self.filename)
        buffer.write(u32(buffer._string_refs[self.filename]))
        buffer.write(u32(len(self.commands)))
        buffer.write(u32(len(self.nodes)))
        buffer.write(u32(len(self.events)))
        buffer.write(u32(len(self.slots)))
        buffer.write(u32(len(self.x38_section)))
        tag_groups = []
        body_sizes = {}
        for command in self.commands:
            if "Tags" in command:
                if command["Tags"] not in tag_groups:
                    tag_groups.append(command["Tags"])
        event_count = 0
        x38 = 0
        body_tags = []
        x38_entries = []
        for node in self.nodes:
            if "Tags" in self.nodes[node]:
                if self.nodes[node]["Tags"] not in tag_groups:
                    tag_groups.append(self.nodes[node]["Tags"])
            for entry in self.nodes[node]["0x38 Entries"]:
                x38_entries.append(entry)
            if self.nodes[node]["Node Type"] == "PreviousTagSelector":
                for child in self.nodes[node]["Body"]["Child Nodes"]:
                    if child["Tags"] and child["Tags"] not in tag_groups:
                        body_tags.append(child["Tags"])
            if self.nodes[node]["Node Type"] == "Event":
                event_count += 1
            if self.nodes[node]["Node Type"] == "InitialFrame":
                if "Tags" in self.nodes[node]["Body"] and self.nodes[node]["Body"]["Tags"] not in tag_groups:
                    body_tags.append(self.nodes[node]["Body"]["Tags"])
            x38 += len(self.nodes[node]["0x38 Entries"])
            body_sizes[node] = self.CalcBodySize(self.nodes[node], self.version)
        for tag in body_tags:
            if tag not in tag_groups:
                tag_groups.append(tag)
        offsets, tag_map, event_offsets = self.CalcOffsets(body_sizes, event_count, x38, tag_groups, buffer)
        buffer.write(u32(offsets["Local Blackboard"]))
        buffer.write(u32(offsets["Strings"]))
        buffer.write(u32(offsets["Enum"]))
        buffer.write(u32(offsets["0x2C"]))
        buffer.write(u32(offsets["Event Offsets"]))
        buffer.write(u32(offsets["Slots"]))
        buffer.write(u32(offsets["0x38"]))
        buffer.write(u32(offsets["0x38 Indices"]))
        buffer.write(u32(offsets["0x40"]))
        buffer.write(u32(len(self.x40_section)))
        buffer.write(u32(offsets["Bone Groups"]))
        buffer.write(u32(len(self.bone_groups)))
        buffer.write(u32(0)) # string pool size to be written to later
        buffer.write(u32(offsets["Transitions"]))
        buffer.write(u32(offsets["Tag List"]))
        buffer.write(u32(offsets["ASMarkings"]))
        buffer.write(u32(offsets["EXB"]))
        buffer.write(u32(offsets["Command Groups"]))
        if self.version == 0x417:
            buffer.write(u32(offsets["0x68"]))
        for command in self.commands:
            buffer.add_string(command["Name"])
            buffer.write(u32(buffer._string_refs[command["Name"]]))
            if "Tags" in command:
                for tag in command["Tags"]:
                    buffer.add_string(tag)
                buffer.write(u32(tag_map[tuple(command["Tags"])]))
            else:
                buffer.write(u32(0))
            self.WriteParameter(buffer, command["Unknown 1"])
            self.WriteParameter(buffer, command["Unknown 2"])
            buffer.write(u32(command["Unknown 3"]))
            # Scuffed but works
            parts = command["GUID"].split('-')
            parts = [int(i, 16) for i in parts]
            buffer.write(u32(parts[0]))
            buffer.write(u16(parts[1]))
            buffer.write(u16(parts[2]))
            buffer.write(u16(parts[3]))
            parts[4] = hex(parts[4])[2:]
            while len(parts[4]) < 12:
                parts[4] = "0" + parts[4]
            buffer.write(byte_custom(bytes.fromhex(parts[4]), 6))
            buffer.write(u16(command["Left Node Index"]))
            buffer.write(u16(command["Right Node Index"] + 1))
        body_offset = offsets["Node Bodies"]
        x40_index = 0
        x3c_index = 0
        for index in self.nodes:
            buffer.write(u16(NodeType[self.nodes[index]["Node Type"]].value))
            buffer.write(u8(len(self.nodes[index]["0x38 Entries"])))
            buffer.write(u8(self.nodes[index]["Unknown"]))
            if "Tags" in self.nodes[index]:
                for tag in self.nodes[index]["Tags"]:
                    buffer.add_string(tag)
                buffer.write(u32(tag_map[tuple(self.nodes[index]["Tags"])]))
            else:
                buffer.write(u32(0))
            buffer.write(u32(body_offset))
            body_offset += body_sizes[index]
            buffer.write(u16(x40_index))
            x40_index += len(self.nodes[index]["0x40 Entries"])
            buffer.write(u16(len(self.nodes[index]["0x40 Entries"])))
            buffer.write(u16(x3c_index))
            x3c_index += len(self.nodes[index]["0x38 Entries"])
            if "ASMarkings" in self.nodes[index]:
                buffer.write(u16(self.as_markings.index(self.nodes[index]["ASMarkings"]) + 1))
            else:
                buffer.write(u16(0))
            # Scuffed but works
            parts = self.nodes[index]["GUID"].split('-')
            parts = [int(i, 16) for i in parts]
            buffer.write(u32(parts[0]))
            buffer.write(u16(parts[1]))
            buffer.write(u16(parts[2]))
            buffer.write(u16(parts[3]))
            parts[4] = hex(parts[4])[2:]
            while len(parts[4]) < 12:
                parts[4] = "0" + parts[4]
            buffer.write(byte_custom(bytes.fromhex(parts[4]), 6))
        for offset in event_offsets:
            buffer.write(u32(offset))
        event_index = 0
        for index in self.nodes:
            if "Body" in self.nodes[index]:
                body = self.nodes[index]["Body"]
                if self.nodes[index]["Node Type"] == "FloatSelector":
                    self.WriteParameter(buffer, body["Parameter"])
                    self.WriteParameter(buffer, body["Unknown 1"])
                    buffer.write(u32(1 if body["Unknown 2"] else 0))
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "StringSelector":
                    self.WriteParameter(buffer, body["Parameter"])
                    self.WriteParameter(buffer, body["Unknown 1"])
                    buffer.write(u32(1 if body["Unknown 2"] else 0))
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "SkeletalAnimation":
                    self.WriteParameter(buffer, body["Animation"])
                    buffer.write(u32(body["Unknown 1"]))
                    buffer.write(u32(body["Unknown 2"]))
                    self.WriteParameter(buffer, body["Unknown 3"])
                    self.WriteParameter(buffer, body["Unknown 4"])
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "State":
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "OneDimensionalBlender":
                    self.WriteParameter(buffer, body["Parameter"])
                    buffer.write(u32(body["Unknown"]))
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "Sequential":
                    self.WriteParameter(buffer, body["Unknown 1"])
                    self.WriteParameter(buffer, body["Unknown 2"])
                    self.WriteParameter(buffer, body["Unknown 3"])
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "IntSelector":
                    self.WriteParameter(buffer, body["Parameter"])
                    self.WriteParameter(buffer, body["Unknown 1"])
                    buffer.write(u32(1 if body["Unknown 2"] else 0))
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "Simultaneous":
                    buffer.write(u32(body["Unknown"]))
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "Event":
                    buffer.write(u32(event_index))
                    event_index += 1
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "MaterialAnimation":
                    buffer.write(u32(body["Unknown 1"]))
                    self.WriteParameter(buffer, body["Animation"])
                    self.WriteParameter(buffer, body["Unknown 2"])
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "FrameController":
                    self.WriteParameter(buffer, body["Animation Rate"])
                    self.WriteParameter(buffer, body["Start Frame"])
                    self.WriteParameter(buffer, body["End Frame"])
                    buffer.write(u32(body["Unknown Flag"]))
                    self.WriteParameter(buffer, body["Loop Cancel Flag"])
                    self.WriteParameter(buffer, body["Unknown 2"])
                    self.WriteParameter(buffer, body["Unknown 3"])
                    self.WriteParameter(buffer, body["Unknown 4"])
                    self.WriteParameter(buffer, body["Unknown 5"])
                    self.WriteParameter(buffer, body["Unknown 6"])
                    self.WriteParameter(buffer, body["Unknown 7"])
                    self.WriteParameter(buffer, body["Unknown 8"])
                    buffer.write(u32(1 if body["Unknown 9"] else 0))
                    self.WriteParameter(buffer, body["Unknown 10"])
                    if self.version == 0x417:
                        self.WriteParameter(buffer, body["Unknown 11"])
                    buffer.write(u32(body["Unknown 12"]))
                    buffer.write(u32(body["Unknown 13"]))
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "DummyAnimation":
                    self.WriteParameter(buffer, body["Frame"])
                    self.WriteParameter(buffer, body["Unknown"])
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "RandomSelector":
                    buffer.write(u32(body["Unknown 1"]))
                    self.WriteParameter(buffer, body["Unknown 2"])
                    self.WriteParameter(buffer, body["Unknown 3"])
                    buffer.write(u32(1 if body["Unknown 4"] else 0))
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "PreviousTagSelector":
                    buffer.write(u32(body["Unknown"]))
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"], tag_map)
                elif self.nodes[index]["Node Type"] == "BonePositionSelector":
                    self.WriteParameter(buffer, body["Bone 1"])
                    self.WriteParameter(buffer, body["Bone 2"])
                    buffer.write(u32(body["Unknown 1"]))
                    buffer.write(u32(body["Unknown 2"]))
                    self.WriteParameter(buffer, body["Unknown 3"])
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "BoneAnimation":
                    self.WriteParameter(buffer, body["Animation"])
                    self.WriteParameter(buffer, body["Unknown 1"])
                    buffer.write(u32(body["Unknown 2"]))
                    buffer.write(u32(body["Unknown 3"]))
                    self.WriteParameter(buffer, body["Unknown 4"])
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "InitialFrame":
                    buffer.write(u32(body["Flag"]))
                    if "Tags" in body:
                        buffer.write(u32(tag_map[tuple(body["Tags"])]))
                    else:
                        buffer.write(u32(0))
                    self.WriteParameter(buffer, body["Unknown 1"])
                    self.WriteParameter(buffer, body["Bone 1"])
                    self.WriteParameter(buffer, body["Bone 2"])
                    buffer.write(u32(body["Unknown 2"]))
                    self.WriteParameter(buffer, body["Unknown 3"])
                    self.WriteParameter(buffer, body["Unknown 4"])
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "BoneBlender":
                    self.WriteParameter(buffer, body["Bone Group"]["Name"])
                    buffer.write(u32(body["Unknown 1"]))
                    self.WriteParameter(buffer, body["Unknown 2"])
                    buffer.write(u32(body["Unknown 3"]))
                    buffer.write(u32(body["Unknown 4"]))
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "BoolSelector":
                    self.WriteParameter(buffer, body["Parameter"])
                    self.WriteParameter(buffer, body["Unknown 1"])
                    buffer.write(u32(1 if body["Unknown 2"] else 0))
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "Alert":
                    self.WriteParameter(buffer, body["Message"])
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "SubtractAnimation":
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "ShapeAnimation":
                    self.WriteParameter(buffer, body["Animation"])
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                elif self.nodes[index]["Node Type"] == "Unknown7":
                    self.WriteConnections(buffer, body, self.nodes[index]["Node Type"])
                else:
                    raise ValueError(f"Invalid node type {self.nodes[index]['Node Type']}")
        for entry in x38_entries:
            buffer.write(u32(self.x38_section.index(entry)))
        value_offset = offsets["0x38"] + 0x18 * len(self.x38_section)
        for entry in self.x38_section:
            buffer.write(u32(entry["Type"]))
            buffer.write(u32(value_offset))
            if entry["Type"] == 0:
                value_offset += 12
            elif entry["Type"] == 1:
                value_offset += 24
            # Scuffed but works
            parts = entry["GUID"].split('-')
            parts = [int(i, 16) for i in parts]
            buffer.write(u32(parts[0]))
            buffer.write(u16(parts[1]))
            buffer.write(u16(parts[2]))
            buffer.write(u16(parts[3]))
            parts[4] = hex(parts[4])[2:]
            while len(parts[4]) < 12:
                parts[4] = "0" + parts[4]
            buffer.write(byte_custom(bytes.fromhex(parts[4]), 6))
        for entry in self.x38_section:
            if entry["Type"] == 0:
                self.WriteParameter(buffer, entry["Entry"]["Start Frame"])
                buffer.write(u32(entry["Entry"]["Unknown 2"]))
            elif entry["Type"] == 1:
                self.WriteParameter(buffer, entry["Entry"]["Start Frame"])
                self.WriteParameter(buffer, entry["Entry"]["End Frame"])
                self.WriteParameter(buffer, entry["Entry"]["Unknown 3"])
        buffer.write(u32(len(self.x2c_section)))
        for entry in self.x2c_section:
            buffer.write(u16(entry["Source Node"]))
            buffer.write(u16(entry["Target Node"]))
            buffer.write(u32(entry["Unknown 1"]))
            buffer.write(u32(entry["Unknown 2"]))
            buffer.write(u32(entry["Unknown 3"]))
            for subentry in entry["Entries"]:
                buffer.write(u16(subentry["Entry Type"]))
                buffer.write(u16(subentry["Unknown Type"]))
                if subentry["Entry Type"]:
                    self.WriteParameter(buffer, subentry["Unknown 1"])
                    self.WriteParameter(buffer, subentry["Unknown 2"])
                else:
                    buffer.write(u64(0))
                    buffer.write(u64(0))
        for event in self.events:
            buffer.write(u32(len(event["Trigger Events"])))
            buffer.write(u32(len(event["Hold Events"])))
            offset = buffer.tell() + 0x18 * len(event["Trigger Events"]) + 0x1c * len(event["Hold Events"])
            for trigger in event["Trigger Events"]:
                buffer.add_string(trigger["Name"])
                buffer.write(u32(buffer._string_refs[trigger["Name"]]))
                buffer.write(u32(trigger["Unknown 1"]))
                buffer.write(u32(offset))
                offset += 4 + 4 * len(trigger["Parameters"])
                buffer.write(u32(8 * len(trigger["Parameters"])))
                buffer.write(u32(int(trigger["Unknown Hash"], 16)))
                buffer.write(f32(trigger["Start Frame"]))
            for hold in event["Hold Events"]:
                buffer.add_string(hold["Name"])
                buffer.write(u32(buffer._string_refs[hold["Name"]]))
                buffer.write(u32(hold["Unknown 1"]))
                buffer.write(u32(offset))
                offset += 4 + 4 * len(hold["Parameters"])
                buffer.write(u32(8 * len(hold["Parameters"])))
                buffer.write(u32(int(hold["Unknown Hash"], 16)))
                buffer.write(f32(hold["Start Frame"]))
                buffer.write(f32(hold["End Frame"]))
            for trigger in event["Trigger Events"]:
                buffer.write(u32(len(trigger["Parameters"])))
                for param in trigger["Parameters"]:
                    if type(param) == str:
                        flag = 0x40 << 24
                    elif type(param) == float:
                        flag = 0x30 << 24
                    elif type(param) == int:
                        flag = 0x20 << 24
                    elif type(param) == bool:
                        flag = 0x10 << 24
                    else:
                        raise ValueError(param)
                    buffer.write(u32(offset | flag))
                    offset += 8
            for hold in event["Hold Events"]:
                buffer.write(u32(len(hold["Parameters"])))
                for param in hold["Parameters"]:
                    if type(param) == str:
                        flag = 0x40 << 24
                    elif type(param) == float:
                        flag = 0x30 << 24
                    elif type(param) == int:
                        flag = 0x20 << 24
                    elif type(param) == bool:
                        flag = 0x10 << 24
                    else:
                        raise ValueError(param)
                    buffer.write(u32(offset | flag))
                    offset += 8
            for trigger in event["Trigger Events"]:
                for param in trigger["Parameters"]:
                    self.WriteParameter(buffer, param)
            for hold in event["Hold Events"]:
                for param in hold["Parameters"]:
                    self.WriteParameter(buffer, param)
        buffer.write(u32(len(self.transitions)))
        buffer.write(u32(0)) # tf does this do
        offset = buffer.tell() + 0xc * len(self.transitions)
        for transition in self.transitions:
            buffer.write(u32(len(transition["Transitions"])))
            buffer.write(s32(transition["Unknown"]))
            buffer.write(u32(offset))
            offset += 0x20 * len(transition["Transitions"])
        for transition in self.transitions:
            for entry in transition["Transitions"]:
                buffer.add_string(entry["Command 1"])
                buffer.write(u32(buffer._string_refs[entry["Command 1"]]))
                buffer.add_string(entry["Command 2"])
                buffer.write(u32(buffer._string_refs[entry["Command 2"]]))
                if entry["Parameter Type"] == "int":
                    buffer.write(u8(0))
                elif entry["Parameter Type"] == "string":
                    buffer.write(u8(1))
                elif entry["Parameter Type"] == "float":
                    buffer.write(u8(2))
                elif entry["Parameter Type"] == "bool":
                    buffer.write(u8(3))
                elif entry["Parameter Type"] == "vec3f":
                    buffer.write(u8(4))
                else:
                    raise ValueError(f"Invalid parameter type {entry['Parameter Type']}")
                buffer.write(u8(1 if entry["Allow Multiple Matches"] else 0))
                if "Command Group" in entry:
                    buffer.write(u16(self.command_groups.index(entry["Command Group"]) + 1))
                else:
                    buffer.write(u16(0))
                buffer.add_string(entry["Parameter"])
                buffer.write(u32(buffer._string_refs[entry["Parameter"]]))
                self.WriteParameter(buffer, entry["Value"])
                if entry["Parameter Type"] != "vec3f":
                    buffer.write(u64(0))
        if self.command_groups:
            buffer.write(u32(len(self.command_groups)))
            offset = buffer.tell() + 4 * len(self.command_groups)
            for group in self.command_groups:
                buffer.write(u32(offset))
                offset += 4 + len(group) * 4
            for group in self.command_groups:
                buffer.write(u32(len(group)))
                for cmd in group:
                    buffer.add_string(cmd)
                    buffer.write(u32(buffer._string_refs[cmd]))
        if self.local_blackboard_params:
            index = 0
            pos = 0
            for t in type_param:
                if t in self.local_blackboard_params:
                    buffer.write(u16(len(self.local_blackboard_params[t])))
                else:
                    buffer.write(u16(0))
                buffer.write(u16(index))
                if t in self.local_blackboard_params:
                    index += len(self.local_blackboard_params[t])
                if t == "vec3f" and "vec3f" in self.local_blackboard_params:
                    buffer.write(u16(pos))
                    pos = pos + len(self.local_blackboard_params[t]) * 12
                elif t in self.local_blackboard_params:
                    buffer.write(u16(pos))
                    pos = pos + len(self.local_blackboard_params[t]) * 4
                else:
                    buffer.write(u16(pos))
                buffer.write(u16(0))
            files = []
            for t in self.local_blackboard_params:
                for entry in self.local_blackboard_params[t]:
                    buffer.add_string(entry["Name"])
                    name_offset = buffer._string_refs[entry["Name"]]
                    if "File Reference" in entry:
                        if entry["File Reference"] not in files:
                            files.append(entry["File Reference"])
                        name_offset = name_offset | (1 << 31)
                        name_offset = name_offset | (files.index(entry["File Reference"]) << 24)
                    buffer.write(u32(name_offset))
            start = buffer.tell()
            size = 0
            for t in self.local_blackboard_params:
                for entry in self.local_blackboard_params[t]:
                    if t == "int":
                        buffer.write(u32(entry["Init Value"]))
                        size += 4
                    if t == "float":
                        buffer.write(f32(entry["Init Value"]))
                        size += 4
                    if t == "bool":
                        buffer.write(u32(int(entry["Init Value"])))
                        size += 4
                    if t == "vec3f":
                        buffer.write(f32(entry["Init Value"][0]))
                        buffer.write(f32(entry["Init Value"][1]))
                        buffer.write(f32(entry["Init Value"][2]))
                        size += 12
                    if t == "string":
                        buffer.add_string(entry["Init Value"])
                        buffer.write(u32(buffer._string_refs[entry["Init Value"]]))
                        size += 4
            buffer.seek(start + size)
            for file in files:
                buffer.add_string(file["Filename"])
                buffer.write(u32(buffer._string_refs[file["Filename"]]))
                buffer.write(u32(mmh3.hash(file["Filename"], signed=False)))
                buffer.write(u32(mmh3.hash(os.path.splitext(os.path.basename(file["Filename"]))[0], signed=False)))
                buffer.write(u32(mmh3.hash(os.path.splitext(file["Filename"])[1].replace('.', ''), signed=False)))        
        else:
            buffer.skip(48)
        for entry in self.slots:
            buffer.write(u16(len(entry["Entries"])))
            buffer.write(u16(entry["Unknown"]))
            buffer.add_string(entry["Partial 1"])
            buffer.add_string(entry["Partial 2"])
            buffer.write(u32(buffer._string_refs[entry["Partial 1"]]))
            buffer.write(u32(buffer._string_refs[entry["Partial 2"]]))
            for slot in entry["Entries"]:
                buffer.add_string(slot["Bone"])
                buffer.write(u32(buffer._string_refs[slot["Bone"]]))
                buffer.write(u16(slot["Unknown 1"]))
                buffer.write(u16(slot["Unknown 2"]))
        offset = buffer.tell() + 0x10 * len(self.bone_groups)
        for group in self.bone_groups:
            buffer.write(u32(offset))
            offset += 8 * len(group["Bones"])
            buffer.add_string(group["Name"])
            buffer.write(u32(buffer._string_refs[group["Name"]]))
            buffer.write(u32(len(group["Bones"])))
            buffer.write(u32(group["Unknown"]))
        for group in self.bone_groups:
            for bone in group["Bones"]:
                buffer.add_string(bone["Name"])
                buffer.write(u32(buffer._string_refs[bone["Name"]]))
                buffer.write(f32(bone["Unknown"]))
        for entry in self.x40_section:
            buffer.write(u32(entry["Unknown 1"]))
            buffer.write(f32(entry["Angle"]))
            buffer.write(u32(entry["Type"]))
            buffer.write(f32(entry["Unknown 2"]))
            buffer.write(f32(entry["Rate"]))
            buffer.write(f32(entry["Unknown 3"]))
            buffer.write(f32(entry["Min"]))
            buffer.write(f32(entry["Max"]))
        buffer.write(u32(len(self.tag_list)))
        for tag in self.tag_list:
            buffer.add_string(tag)
            buffer.write(u32(buffer._string_refs[tag]))
        for group in tag_groups:
            buffer.write(u32(len(group)))
            for tag in group:
                buffer.add_string(tag)
                buffer.write(u32(buffer._string_refs[tag]))  
        buffer.seek(offsets["ASMarkings"])
        buffer.write(u32(len(self.as_markings)))
        for triplet in self.as_markings:
            for string in triplet:
                buffer.add_string(string)
                buffer.write(u32(buffer._string_refs[string]))
        buffer.write(u32(len(self.x68_section)))
        for entry in self.x68_section:
            buffer.add_string(entry["Name"])
            buffer.write(u32(buffer._string_refs[entry["Name"]]))
            buffer.write(f32(entry["Unknown"]))
        buffer.write(u32(len(self.enum_resolve)))
        for entry in self.enum_resolve:
            buffer.write(u32(entry))
            buffer.add_string(self.enum_resolve[entry]["Class Name"])
            buffer.add_string(self.enum_resolve[entry]["Value Name"])
            buffer.write(u32(buffer._string_refs[self.enum_resolve[entry]["Class Name"]]))
            buffer.write(u32(buffer._string_refs[self.enum_resolve[entry]["Value Name"]]))
        buffer.write(buffer._strings)
        buffer.seek(0x50)
        buffer.write(u32(len(buffer._strings)))
    
    def ToJson(self, output_dir=''):
        if output_dir:
            os.makedirs(output_dir, exist_ok=True)
        with open(os.path.join(output_dir, self.filename + ".json"), 'w', encoding='utf-8') as f:
            json.dump(self.output_dict, f, indent=4, ensure_ascii=False)

def asb_from_zs(filepath, romfs_path=''):
    zs = zstd.Zstd(romfs_path)
    return ASB(zs.Decompress(filepath, no_output=True))

def asb_to_json(asb_path, output_dir='', romfs_path=''):
    if os.path.splitext(asb_path)[1] in ['.zs', '.zstd']:
        file = asb_from_zs(asb_path, romfs_path)
    else:
        file = ASB(asb_path)
    file.ToJson(output_dir)

def json_to_asb(json_path, output_dir='', compress=False, romfs_path=''):
    file = ASB(json_path)
    file.ToBytes(output_dir)
    if compress:
        zs = zstd.Zstd(romfs_path)
        zs.Compress(os.path.join(output_dir, file.filename + ".asb"), output_dir)
        os.remove(os.path.join(output_dir, file.filename + ".asb"))

if __name__ == "__main__":
    json_to_asb("Drake.root.json")