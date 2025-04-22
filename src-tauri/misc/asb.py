try:
    from exb import EXB
except ImportError:
    raise ImportError("exb.py not found")
try:
    from utils import *
except ImportError:
    raise ImportError("utils.py not found")
try:
    from baev import *
except ImportError:
    raise ImportError("baev.py not found")

from enum import Enum
import json
import os
try:
    import mmh3
except ImportError:
    raise ImportError("mmh3 not found (pip install mmh3)")

# Node types
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

class Mode(Enum):
    Generic                 = 0
    Degrees                 = 1
    Radians                 = 2
    DegreesNormalize        = 3
    RadiansNormalize        = 4

class SelectFlag(Enum):
    NoSelectOnUpdate        = 0
    SelectOnUpdate          = 1
    SelectOnUpdateNoFadeout = 2

class Axis(Enum):
    X                       = 0
    Y                       = 1
    Z                       = 2

# please take these with a grain of salt :)
class InitialFrameCalcMode(Enum):
    ReturnStart             = 0
    NormCurrent             = 1
    NormCurrentComplement   = 2
    AllLoops                = 3
    BoneComparison          = 4
    ReturnEnd               = 5
    Current                 = 6
    CurrentComplement       = 7
    NormSingleLoop          = 8

class StateCheckType(Enum):
    CheckStateAndNodeMatch          = 0
    CheckState                      = 1 # always does the target node and never next if valid
    CheckStateFinishAndNodeMatch    = 2

class CompareOperator(Enum):
    GreaterThan         = 0
    LessThanEquals      = 1
    Equals              = 2
    NotEquals           = 3
    GreaterThanEquals   = 4
    LessThan            = 5

# Parameter order for blackboard
blackboard_types = ["string", "int", "float", "bool", "vec3f", "ptr"]

class Blackboard:
    def __init__(self, stream, string_pool):
        self.stream = stream
        self.string_pool = string_pool
        self.max_index = 0
        self.blackboard = {}
        header = {}
        for type in blackboard_types:
            header[type] = self.read_header()
        for type in blackboard_types:
            self.blackboard[type] = []
            for i in range(header[type]["Count"]):
                self.blackboard[type].append(self.read_entry())
        base = self.stream.tell()
        for type in blackboard_types:
            self.stream.seek(base + header[type]["Offset"])
            for param in self.blackboard[type]:
                param["Init Value"] = self.read_value(type)
        external_files = []
        for i in range(self.max_index + 1):
            external_files.append(self.read_file_ref())
        for type in self.blackboard:
            for param in self.blackboard[type]:
                if "Index" in param:
                    param["Reference File"] = external_files[param["Index"]]
                    del param["Index"]
        self.blackboard = {k: v for k, v in self.blackboard.items() if v}

    def read_header(self):
        entry = {}
        entry["Count"] = self.stream.read_u16()
        entry["Index"] = self.stream.read_u16()
        entry["Offset"] = self.stream.read_u16()
        self.stream.read(2)
        return entry
    
    def read_entry(self):
        entry = {}
        flags = self.stream.read_u32()
        valid_index = bool(flags >> 31)
        if valid_index:
            entry["Index"] = (flags >> 24) & 0b1111111
            if entry["Index"] > self.max_index:
                self.max_index = entry["Index"]
        name_offset = flags & 0x3FFFFF
        entry["Name"] = self.string_pool.read_string(name_offset)
        return entry
    
    def read_file_ref(self):
        filename = self.string_pool.read_string(self.stream.read_u32())
        self.stream.read(12) # these are just hashes of different parts of the filename we can recalculate later
        return filename
    
    def read_value(self, datatype):
        if datatype == "int":
            value = self.stream.read_u32()
        if datatype == "bool":
            value = bool(self.stream.read_u32())
        if datatype == "float":
            value = self.stream.read_f32()
        if datatype == "string":
            value = self.string_pool.read_string(self.stream.read_u32())
        if datatype == "vec3f":
            value = [self.stream.read_f32(), self.stream.read_f32(), self.stream.read_f32()]
        if datatype == "ptr":
            value = None
        return value

class ASB:
    def __init__(self, data, stream=ReadStream(b''), string_pool=ReadStream(b'')):
        if data:
            self.filename = data["Metadata"]["Filename"]
            self.version = int(data["Metadata"]["Version"], 16)
            self.has_asnode_baev = data["Metadata"]["HasASNodeBaev"]
            self.commands = data["Commands"]
            self.nodes = data["Nodes"]
            self.blackboard = data["Blackboard"]
            self.bone_groups = data["Bone Groups"]
            self.expressions = data["Expressions"]
            self.transitions = data["Transitions"]
            self.valid_tags = data["Valid Tags"]
            self.partials = data["Partials"]
        else:
            self.filename = ""
            self.version = 0
            self.has_asnode_baev = False
            self.commands = []
            self.nodes = []
            self.blackboard = {}
            self.bone_groups = []
            self.expressions = []
            self.transitions = []
            self.valid_tags = []
            self.partials = []
            self.material_blend = []
        self.stream: ReadStream = stream
        self.string_pool: ReadStream = string_pool
        self.calc_ctrl = []
        self.events = []
        self.sync_ctrl = []
        self.command_groups = []
        self.as_markings = []
        self.state_transitions = []
        self.material_blend = []

        for node in self.nodes:
            if "Calc Controllers" in node:
                self.calc_ctrl += node["Calc Controllers"]
            if "Sync Controls" in node:
                self.sync_ctrl += node["Sync Controls"]
            if node["Node Type"] == "Event":
                self.events.append(node["Body"]["Event"])
            if "ASMarking" in node:
                if node["ASMarking"] not in self.as_markings:
                    self.as_markings.append(node["ASMarking"])
            if node["Node Type"] == "MaterialAnimation":
                if "Material Blend Setting" in node["Body"]:
                    if node["Body"]["Material Blend Setting"] not in self.material_blend:
                        self.material_blend.append(node["Body"]["Material Blend Setting"])
            if "Body" in node and "State Transitions" in node["Body"]:
                for transition in node["Body"]["State Transitions"]:
                    if transition["State Transition"]:
                        self.state_transitions.append(transition["State Transition"])
        
        for transition_group in self.transitions:
            for transition in transition_group["Transitions"]:
                if "Command Group" in transition and transition["Command Group"] not in self.command_groups:
                    self.command_groups.append(transition["Command Group"])

    @classmethod
    def from_binary(cls, data):
        assert type(data) in [bytes, bytearray], "Data should be bytes or bytearray"
        stream = ReadStream(data)

        magic = stream.read(4)
        assert magic == b'ASB ', f"Invalid file magic '{magic.decode('utf-8')}', expected 'ASB '"
        version = stream.read_u32()
        assert version == 0x417, f"Unsupported version {hex(version)}, expected 0x417"

        filename_offset = stream.read_u32()
        command_count = stream.read_u32()
        node_count = stream.read_u32()
        event_count = stream.read_u32()
        partial_count = stream.read_u32()
        sync_control_count = stream.read_u32()
        blackboard_offset = stream.read_u32()
        string_pool_offset = stream.read_u32()
        
        pos = stream.tell()
        stream.seek(string_pool_offset)
        string_pool = ReadStream(stream.read())
        stream.seek(pos)
        this = cls(None, stream, string_pool)
        this.filename = this.string_pool.read_string(filename_offset)
        this.version = version

        enum_resolve_offset = this.stream.read_u32() # section is identical to AINB and also unused so I won't bother
        state_transition_offset = this.stream.read_u32()
        event_offsets_offset = this.stream.read_u32()
        partials_offset = this.stream.read_u32()
        sync_control_offset = this.stream.read_u32()
        sync_control_indices_offset = this.stream.read_u32()
        calc_controller_offset = this.stream.read_u32()
        # this count doesn't line up...
        # might be because some controllers calculate the same value but idk how to tell
        calc_controller_count = this.stream.read_u32()
        bone_groups_offset = this.stream.read_u32()
        bone_group_count = this.stream.read_u32()
        string_pool_size = this.stream.read_u32()
        transitions_offset = this.stream.read_u32()
        valid_tags_offset = this.stream.read_u32()
        as_markings_offset = this.stream.read_u32()
        expression_offset = this.stream.read_u32()
        command_groups_offset = this.stream.read_u32()
        material_blend_offset = this.stream.read_u32()
        assert this.stream.tell() == 0x6C, f"Invalid header size {this.stream.tell()}, should be 0x6C"

        commands_offset = this.stream.tell()

        this.stream.seek(blackboard_offset)
        this.blackboard = Blackboard(this.stream, this.string_pool).blackboard

        if expression_offset != 0:
            this.stream.seek(expression_offset)
            this.expressions = EXB(this.stream.read()).exb_section
        
        this.stream.seek(calc_controller_offset)
        while this.stream.tell() < valid_tags_offset:
            this.calc_ctrl.append(this.read_calc_ctrl())

        this.stream.seek(commands_offset)
        for i in range(command_count):
            this.commands.append(this.read_command())

        node_offset = this.stream.tell()
        assert node_offset == 0x6C + command_count * 0x30, f"Error reading commands"

        this.stream.seek(event_offsets_offset)
        for i in range(event_count):
            this.events.append(this.read_event())

        this.stream.seek(sync_control_offset)
        for i in range(sync_control_count):
            this.sync_ctrl.append(this.read_sync_control())

        this.stream.seek(bone_groups_offset)
        for i in range(bone_group_count):
            this.bone_groups.append(this.read_bone_group())
        
        if command_groups_offset != 0:
            this.stream.seek(command_groups_offset)
            for i in range(this.stream.read_u32()):
                this.command_groups.append(this.read_command_group())
        
        this.stream.seek(transitions_offset)
        count = this.stream.read_u32()
        stream.read(4) # usually 0 idk what it's for
        for i in range(count):
            this.transitions.append(this.read_transition_group())
        
        this.stream.seek(valid_tags_offset)
        for i in range(this.stream.read_u32()):
            this.valid_tags.append(this.string_pool.read_string(this.stream.read_u32()))
        
        this.stream.seek(partials_offset)
        for i in range(partial_count):
            this.partials.append(this.read_partial())
        
        this.stream.seek(as_markings_offset)
        for i in range(this.stream.read_u32()):
            this.as_markings.append(this.read_as_marking())
        
        this.stream.seek(material_blend_offset)
        for i in range(this.stream.read_u32()):
            this.material_blend.append(this.read_material_blend())
        
        this.stream.seek(state_transition_offset)
        for i in range(this.stream.read_u32()):
            this.state_transitions.append(this.read_state_transition())

        this.stream.seek(node_offset)
        for i in range(node_count):
            this.nodes.append(this.read_node(sync_control_indices_offset))

        return this

    @classmethod
    def from_dict(cls, data):
        assert type(data) == dict, "Data should be a dictionary"
        return cls(data)
    
    def asdict(self):
        return {
            "Metadata" : {"Filename" : self.filename, "Version" : hex(self.version),
                          "HasASNodeBaev" : self.has_asnode_baev},
            "Blackboard" : self.blackboard,
            "Bone Groups" : self.bone_groups,
            "Expressions" : self.expressions,
            "Partials" : self.partials,
            "Transitions" : self.transitions,
            "Valid Tags" : self.valid_tags,
            "Commands" : self.commands,
            "Nodes" : self.nodes
        }

    def read_calc_ctrl(self):
        controller = {}
        index = self.stream.read_s32()
        if index < 0:
            controller["Parameter"] = {"Command Data Type": index & 0xFFFF}
        else:
            controller["Parameter"] = {"Blackboard Index": index & 0xFFFF, "Type": "float"}
        controller["Adjust Value"] = self.stream.read_f32()
        controller["Calc Mode"] = Mode(self.stream.read_u32()).name
        controller["Default Value"] = self.stream.read_f32()
        controller["Adjust Rate"] = self.stream.read_f32()
        controller["Base Result"] = self.stream.read_f32()
        controller["Min"] = self.stream.read_f32()
        controller["Max"] = self.stream.read_f32()
        return controller

    # very fun
    def parse_param(self, type, get_select_flag=False):
        flags = self.stream.read_s32()
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
        if flags < 0:
            orig = value
            flags = flags & 0xFFFFFFFF
            if flags & 0x81000000 == 0x81000000:
                value = {"Expression Index": flags & 0xFFFF, "Input": value}
            else:
                if type == "float":
                    if flags & (1 << 0x1e) != 0:
                        value = self.calc_ctrl[flags & 0xFFFF]
                    else:
                        if flags & (1 << 0x19) != 0:
                            value = {"Command Data Type": flags & 0xFFFF}
                        else:
                            type_flag = (flags >> 0x1a) & 3
                            if type_flag == 0:
                                value = {"Blackboard Index": flags & 0xFFFF, "Type": type}
                            elif type_flag == 1:
                                value = {"Blackboard Index": flags & 0xFFFF, "Type": "vec3f", "Axis": "X"}
                            elif type_flag == 2:
                                value = {"Blackboard Index": flags & 0xFFFF, "Type": "vec3f", "Axis": "Y"}
                            else:
                                value = {"Blackboard Index": flags & 0xFFFF, "Type": "vec3f", "Axis": "Z"}
                elif type == "string":
                    if flags & (1 << 0x19) != 0:
                        value = {"Command Data Type": flags & 0xFFFF}
                    else:
                        value = {"Blackboard Index": flags & 0xFFFF, "Type": type}
                else:
                    value = {"Blackboard Index": flags & 0xFFFF, "Type": type}
            value["Select Flag"] = SelectFlag(flags >> 0x1c & 3).name
            if orig and "Input" not in value:
                value["Default Value"] = orig
        return value

    # needs to be formatted as %08x-%04x-%04x-%02x%02x-%02x%02x%02x%02x%02x%02x for baev hash calculations
    def read_guid(self):
        return "%08x-%04x-%04x-%02x%02x-%02x%02x%02x%02x%02x%02x" \
            % (self.stream.read_u32(), self.stream.read_u16(), self.stream.read_u16(),
               self.stream.read_u8(), self.stream.read_u8(),
               self.stream.read_u8(), self.stream.read_u8(), self.stream.read_u8(),
               self.stream.read_u8(), self.stream.read_u8(), self.stream.read_u8())

    def read_tag_group(self):
        count = self.stream.read_u32()
        tags = []
        for i in range(count):
            tags.append(self.string_pool.read_string(self.stream.read_u32()))
        return tags

    def read_command(self):
        command = {}
        command["Name"] = self.string_pool.read_string(self.stream.read_u32())
        tag_offset = self.stream.read_u32()
        if tag_offset != 0:
            pos = self.stream.tell()
            self.stream.seek(tag_offset)
            command["Tags"] = self.read_tag_group()
            self.stream.seek(pos)
        command["Unknown 1"] = self.parse_param("float")
        command["Ignore Same Command"] = self.parse_param("bool")
        command["Interpolation Type"] = self.stream.read_u32()
        command["GUID"] = self.read_guid()
        command["Node Index"] = self.stream.read_u16()
        self.stream.read(2) # a second node index in AINB but I haven't seen it ever used and it's always 0 so might be padding
        return command
    
    def read_event_param(self):
        count = self.stream.read_u32()
        offsets = []
        for i in range(count):
            offsets.append(self.stream.read_u32())
        params = []
        for offset in offsets:
            type_flag = (offset & 0xFF000000) >> 24
            offset = offset & 0xFFFFFF
            self.stream.seek(offset)
            if type_flag == 0x40:
                params.append(self.parse_param("string"))
            elif type_flag == 0x30:
                params.append(self.parse_param("float"))
            elif type_flag == 0x20:
                params.append(self.parse_param("int"))
            elif type_flag == 0x10:
                params.append(self.parse_param("bool"))
            else:
                raise ValueError(hex(type_flag), hex(offset)) # there might be a vec3f one but I haven't seen it before
        return params
    
    def read_trigger_event(self):
        event = {}
        event["Name"] = self.string_pool.read_string(self.stream.read_u32())
        event["Unknown"] = self.stream.read_u32()
        offset = self.stream.read_u32()
        param_size = self.stream.read_u32()
        event["Hash"] = "0x%08x" % self.stream.read_u32() # not sure what this is a hash of nor how it's hashed
        event["Start Frame"] = self.stream.read_f32()
        pos = self.stream.tell()
        self.stream.seek(offset)
        event["Parameters"] = self.read_event_param()
        self.stream.seek(pos)
        return event

    def read_hold_event(self):
        event = {}
        event["Name"] = self.string_pool.read_string(self.stream.read_u32())
        event["Unknown"] = self.stream.read_u32()
        offset = self.stream.read_u32()
        param_size = self.stream.read_u32()
        event["Hash"] = "0x%08x" % self.stream.read_u32() # not sure what this is a hash of nor how it's hashed
        event["Start Frame"] = self.stream.read_f32()
        event["End Frame"] = self.stream.read_f32()
        pos = self.stream.tell()
        self.stream.seek(offset)
        event["Parameters"] = self.read_event_param()
        self.stream.seek(pos)
        return event

    def read_event(self):
        offset = self.stream.read_u32()
        pos = self.stream.tell()
        self.stream.seek(offset)
        trigger_count = self.stream.read_u32()
        hold_count = self.stream.read_u32()
        events = {}
        if trigger_count > 0:
            events["Trigger Events"] = []
            for i in range(trigger_count):
                events["Trigger Events"].append(self.read_trigger_event())
        if hold_count > 0:
            events["Hold Events"] = []
            for i in range(hold_count):
                events["Hold Events"].append(self.read_hold_event())
        self.stream.seek(pos)
        return events
    
    def read_sync_control(self):
        controller = {}
        type = self.stream.read_u32()
        offset = self.stream.read_u32()
        controller["GUID"] = self.read_guid()
        pos = self.stream.tell()
        self.stream.seek(offset)
        if type == 0:
            controller["Fade In Frame"] = self.parse_param("float")
            controller["Unknown"] = self.stream.read_u32()
        elif type == 1:
            controller["Sync Start Frame"] = self.parse_param("float")
            controller["Normalized Sync Start Frame"] = self.parse_param("float")
            controller["Unknown"] = self.parse_param("float")
        elif type == 3: # used for something but not entirely sure what, no data though
            pass
        else:
            raise ValueError(f"Invalid sync control type {type} @ {hex(pos - 0x18)}")
        self.stream.seek(pos)
        return controller
    
    def read_bone_group(self):
        bone_group = {}
        offset = self.stream.read_u32()
        bone_group["Name"] = self.string_pool.read_string(self.stream.read_u32())
        count = self.stream.read_u32()
        bone_group["Unknown"] = self.stream.read_u32()
        bone_group["Bones"] = []
        pos = self.stream.tell()
        self.stream.seek(offset)
        for i in range(count):
            bone = {}
            bone["Name"] = self.string_pool.read_string(self.stream.read_u32())
            bone["Unknown"] = self.stream.read_f32()
            bone_group["Bones"].append(bone)
        self.stream.seek(pos)
        return bone_group
    
    def read_command_group(self):
        cmd_group = []
        offset = self.stream.read_u32()
        pos = self.stream.tell()
        self.stream.seek(offset)
        count = self.stream.read_u32()
        for i in range(count):
            cmd_group.append(self.string_pool.read_string(self.stream.read_u32()))
        self.stream.seek(pos)
        return cmd_group
    
    def read_transition(self):
        transition = {}
        types = {
            0 : "int",
            1 : "string",
            2 : "float",
            3 : "bool",
            4 : "vec3f"
        }
        transition["Current Command"] = self.string_pool.read_string(self.stream.read_u32())
        transition["Next Command"] = self.string_pool.read_string(self.stream.read_u32())
        enum = self.stream.read_u8()
        if enum in types:
            transition["Parameter Type"] = types[enum]
        else:
            transition["Parameter Type"] = "vec3f"
        transition["Allow Multiple Matches"] = bool(self.stream.read_u8())
        cmd_group_index = self.stream.read_u16() - 1
        transition["Parameter"] = self.string_pool.read_string(self.stream.read_u32())
        transition["Value"] = self.parse_param(transition["Parameter Type"])
        if transition["Parameter Type"] != "vec3f":
            self.stream.read(8)
        if cmd_group_index >= 0:
            transition["Command Group"] = self.command_groups[cmd_group_index]
        return transition
    
    def read_transition_group(self):
        transition = {}
        count = self.stream.read_u32()
        transition["Unknown"] = self.stream.read_s32()
        offset = self.stream.read_u32()
        pos = self.stream.tell()
        self.stream.seek(offset)
        transition["Transitions"] = []
        for i in range(count):
            transition["Transitions"].append(self.read_transition())
        self.stream.seek(pos)
        return transition
    
    def read_as_marking(self):
        return [self.string_pool.read_string(self.stream.read_u32()), # "ASMarking"
                 self.string_pool.read_string(self.stream.read_u32()), # "ASマーキング" (same thing but in jp)
                 self.string_pool.read_string(self.stream.read_u32())] # marking name

    def read_material_blend(self):
        entry = {}
        entry["Name"] = self.string_pool.read_string(self.stream.read_u32())
        entry["Blend Start"] = self.stream.read_f32()
        return entry
    
    def read_state_transition_param(self):
        param = {}
        types = {
            0 : "",
            1 : "float",
            2 : "int",
            3 : "string"
        }
        type = types[self.stream.read_u16()]
        param["Compare Type"] = CompareOperator(self.stream.read_u16()).name
        if type == "": # skips check entirely and only checks if the current node is finished or not
            self.stream.read(16)
        else:
            param["Value 1"] = self.parse_param(type)
            param["Value 2"] = self.parse_param(type)
        return param
    
    def read_state_transition(self):
        transition = {}
        transition["Current Node"] = self.stream.read_u16()
        transition["Target Node"] = self.stream.read_u16()
        transition["Check Type"] = StateCheckType(self.stream.read_u32()).name
        # honestly don't know about these two
        transition["Transition to Next Instead of Target"] = bool(self.stream.read_u8())
        transition["Skip Transition"] = bool(self.stream.read_u8())
        self.stream.read(2)
        transition["Unknown"] = self.stream.read_u32()
        transition["Parameters"] = [
            self.read_state_transition_param(), self.read_state_transition_param(),
            self.read_state_transition_param(), self.read_state_transition_param()
        ]
        return transition
    
    def read_partial(self):
        partial = {}
        count = self.stream.read_u16()
        partial["Is Material Slot"] = bool(self.stream.read_u16())
        partial["Name"] = self.string_pool.read_string(self.stream.read_u32())
        partial["Unknown"] = self.string_pool.read_string(self.stream.read_u32())
        partial["Bones"] = []
        for i in range(count):
            bone = {}
            bone["Name"] = self.string_pool.read_string(self.stream.read_u32())
            bone["Unknown 1"] = self.stream.read_u16()
            bone["Unknown 2"] = self.stream.read_u16()
            partial["Bones"].append(bone)
        return partial
    
    def read_node(self, sync_offset):
        node = {}
        node["Node Index"] = len(self.nodes)
        node["Node Type"] = NodeType(self.stream.read_u16()).name
        sync_count = self.stream.read_u8()
        node["No State Transition"] = bool(self.stream.read_u8())
        tag_offset = self.stream.read_u32()
        if tag_offset != 0:
            pos = self.stream.tell()
            self.stream.seek(tag_offset)
            node["Tags"] = self.read_tag_group()
            self.stream.seek(pos)
        body_offset = self.stream.read_u32()
        calc_ctrl_index = self.stream.read_u16()
        calc_ctrl_count = self.stream.read_u16()
        sync_index = self.stream.read_u16()
        as_marking_index = self.stream.read_u16() - 1
        node["GUID"] = self.read_guid()
        pos = self.stream.tell()
        if as_marking_index >= 0:
            node["ASMarking"] = self.as_markings[as_marking_index]
        if sync_count > 0:
            self.stream.seek(sync_offset + 0x4 * sync_index)
            node["Sync Controls"] = []
            for i in range(sync_count):
                node["Sync Controls"].append(self.sync_ctrl[self.stream.read_u32()])
        if calc_ctrl_count > 0:
            node["Calc Controllers"] = self.calc_ctrl[calc_ctrl_index : calc_ctrl_index + calc_ctrl_count]
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
    
    def read_connections(self):
        offsets = {"State" : [], "Unk" : [], "Child" : [], "State Transition" : [], "Event" : [], "Frame Controls" : []}
        # This type is used by State Connections but I haven't seen any these nodes used ever
        state_count = self.stream.read_u8()
        state_index = self.stream.read_u8() # base index, same goes for the other types
        # Appear to be unused as far as I can tell
        unknown_count = self.stream.read_u8()
        unknown_index = self.stream.read_u8()
        child_count = self.stream.read_u8()
        child_index = self.stream.read_u8()
        transition_count = self.stream.read_u8()
        transition_index = self.stream.read_u8()
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
        for i in range(transition_count):
            offsets["State Transition"].append(self.stream.read_u32())
        for i in range(event_count):
            offsets["Event"].append(self.stream.read_u32())
        for i in range(frame_count):
            offsets["Frame Controls"].append(self.stream.read_u32())
        state = []
        if offsets["State"]:
            for offset in offsets["State"]:
                self.stream.seek(offset)
                state.append(self.stream.read_u32())
        transition = []
        if offsets["State Transition"]:
            for offset in offsets["State Transition"]:
                self.stream.seek(offset)
                index = self.stream.read_s32()
                entry = {"State Transition" : {}, "Node Index" : -1}
                if index >= 0:
                    entry["State Transition"] = self.state_transitions[index]
                entry["Node Index"] = self.stream.read_u32()
                transition.append(entry)
        event = []
        if offsets["Event"]:
            for offset in offsets["Event"]:
                self.stream.seek(offset)
                event.append(self.stream.read_u32())
        frame = []
        if offsets["Frame Controls"]:
            for offset in offsets["Frame Controls"]:
                self.stream.seek(offset)
                frame.append(self.stream.read_u32())
        return offsets, transition, event, frame, state
    
    def FloatSelector(self):
        entry = {}
        entry["Parameter"] = self.parse_param("float", True)
        entry["Is Sync"] = self.parse_param("bool")
        entry["Force Run"] = bool(self.stream.read_u32())
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                child = {}
                if len(entry["Child Nodes"]) == len(offsets["Child"]) - 1:
                    child["Default"] = self.parse_param("string")
                    self.stream.read(8) # empty
                    child["Node Index"] = self.stream.read_u32() 
                else:
                    child["Condition Min"] = self.parse_param("float")
                    child["Condition Max"] = self.parse_param("float")
                    child["Node Index"] = self.stream.read_u32()
                entry["Child Nodes"].append(child)
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def StringSelector(self):
        entry = {}
        entry["Parameter"] = self.parse_param("string", True)
        entry["Is Sync"] = self.parse_param("bool")
        entry["Force Run"] = bool(self.stream.read_u32())
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                child = {}
                if len(entry["Child Nodes"]) == len(offsets["Child"]) - 1:
                    child["Default"] = self.parse_param("string")
                    child["Node Index"] = self.stream.read_u32() 
                else:
                    child["Condition"] = self.parse_param("string")
                    child["Node Index"] = self.stream.read_u32()
                entry["Child Nodes"].append(child)
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def SkeletalAnimation(self):
        entry = {}
        entry["Animation"] = self.parse_param("string")
        entry["Unknown 1"] = self.stream.read_u32() # appears to be a bool that controls the same flag
        entry["Unknown 2"] = self.stream.read_u32() # sets some flag
        entry["Unknown 3"] = self.parse_param("bool") # use the float
        entry["Unknown 4"] = self.parse_param("float") # some duration thing
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def State(self):
        entry = {}
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def Unknown2(self):
        return {} # No node body

    def OneDimensionalBlender(self):
        entry = {}
        entry["Parameter"] = self.parse_param("float", True) # also holds the select flag
        # if set to 1, then it does (r^2 * (3.0 - 2r)) to the calculated ratio twice -> this is a cubic smoothstep
        entry["Lerp Mode"] = self.stream.read_u32()
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                child = {}
                child["Condition Min"] = self.parse_param("float")
                child["Condition Max"] = self.parse_param("float")
                child["Node Index"] = self.stream.read_u32()
                entry["Child Nodes"].append(child)
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def Sequential(self):
        entry = {}
        entry["Use Sync Range Mult"] = self.parse_param("bool") # use sync range multiplier
        entry["Sync Range Mult"] = self.parse_param("int") # sync range multiplier
        entry["Unknown 3"] = self.parse_param("int") # Unsure of data type
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def IntSelector(self):
        entry = {}
        entry["Parameter"] = self.parse_param("int", True)
        entry["Is Sync"] = self.parse_param("bool")
        entry["Force Run"] = bool(self.stream.read_u32())
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                child = {}
                if len(entry["Child Nodes"]) == len(offsets["Child"]) - 1:
                    child["Default"] = self.parse_param("int")
                    child["Node Index"] = self.stream.read_u32() 
                else:
                    child["Condition"] = self.parse_param("int")
                    child["Node Index"] = self.stream.read_u32()
                entry["Child Nodes"].append(child)
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def Simultaneous(self):
        entry = {}
        entry["Finish With Child"] = bool(self.stream.read_u32()) # finish if any child finishes?
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def EventNode(self):
        entry = {}
        index = self.stream.read_u32()
        entry["Event"] = self.events[index] # literally a fucking scam!!!!!
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def MaterialAnimation(self):
        entry = {}
        index = self.stream.read_u32() - 1
        if index >= 0:
            entry["Material Blend Setting"] = self.material_blend[index] # 0x68 index (-1 for index)
        entry["Animation"] = self.parse_param("string")
        entry["Is Loop"] = self.parse_param("bool") # loop flag
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def FrameController(self):
        entry = {}
        entry["Animation Rate"] = self.parse_param("float") # 0x00
        entry["Start Frame"] = self.parse_param("float") # 0x08
        entry["End Frame"] = self.parse_param("float") # 0x10
        # not entirely sure what the differences are here
        # values appear to be 0, 1, 2, 3, or 4
        # 1 = loop, 3 = loop
        # 2 = no loop
        # 4 = loop, 0 = loop if anim loop flag set
        entry["Loop Flags"] = self.stream.read_u32() # 0x18
        entry["Loop Cancel Flag"] = self.parse_param("bool") # 0x1c cancels loop when true
        entry["Unknown 2"] = self.parse_param("bool") # 0x24 sets the frame controller flags to | 4 if true
        entry["Loop Num"] = self.parse_param("int") # 0transitions loop count
        entry["Max Random Loop Num"] = self.parse_param("int") # 0x34 bonus loop count (from 0 to the value)
        # if true, it doesn't use the random loop num
        entry["Is Not Use Random Bonus Loop"] = self.parse_param("bool") # 0x3c
        # normalized duration into the animation to freeze at (0.0 is start, 1.0 is end, relative to the specified start/end)
        # basically allows them to "slide" through the animation by changing this value as they wish
        # does not play the animation and just starts at that frame
        entry["Animation Freeze Point"] = self.parse_param("float") # 0x44
        # frame of into the animation to freeze at (plays the animation up until that point)
        entry["Animation Freeze Frame"] = self.parse_param("float") # 0x4c
        # this is for if you want the animation to loop but not for a fixed number of times but rather a fixed duration
        entry["Loop Duration"] = self.parse_param("float") # 0x58 some additional duration thing?
        # whether or not to include the initial loop in the loop duration
        entry["Is Include Initial Loop"] = bool(self.stream.read_u32()) # 0x5c
        entry["Unknown 10"] = self.parse_param("float") # 0x60 seems to be for syncing
        entry["Unknown 11"] = self.parse_param("bool") # 0x68 sets the flag to | 0x400 if true
        entry["Unknown 12"] = self.stream.read_u32() # 0x70 controls the | 0x2000 flag (uint)
        entry["Unknown 13"] = self.stream.read_u32() # 0x74 controls the | 0x1000 flag (bool)
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry
    
    def DummyAnimation(self):
        entry = {}
        entry["Frame"] = self.parse_param("float") # frame count
        entry["Is Loop"] = self.parse_param("bool") # loop flag
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def RandomSelector(self):
        entry = {}
        entry["Select Flag"] = SelectFlag(self.stream.read_u32()).name # select flag
        entry["Is Sync"] = self.parse_param("bool") # sync
        entry["Max Cached Select Count"] = self.parse_param("int") # cached selection count, caps at 4
        entry["Force Run"] = bool(self.stream.read_u32()) # force select
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                child = {}
                child["Weight"] = self.parse_param("float")
                child["Node Index"] = self.stream.read_u32()
                entry["Child Nodes"].append(child)
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def Unknown4(self):
        return {} # No node body

    def PreviousTagSelector(self):
        entry = {}
        entry["Tag Set Index"] = self.stream.read_u32() # tag set (0-3)
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                link = {"Tags" : [], "Node Index" : -1}
                self.stream.seek(offset)
                tag_offset = self.stream.read_u32()
                link["Node Index"] = self.stream.read_u32()
                if tag_offset != 0xFFFFFFFF:
                    self.stream.seek(tag_offset)
                    link["Tags"] = self.read_tag_group()
                entry["Child Nodes"].append(link)
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def BonePositionSelector(self):
        entry = {}
        entry["Bone 1"] = self.parse_param("string") # for pos 1
        entry["Bone 2"] = self.parse_param("string") # subtracted from pos 1 and difference is compared
        entry["Axis"] = Axis(self.stream.read_u32()).name # axis (0 = x, 1 = y, 2 = z)
        entry["Select Flag"] = SelectFlag(self.stream.read_u32()).name # select flag
        entry["Is Sync"] = self.parse_param("bool") # is update sync frame
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                child = {}
                if len(entry["Child Nodes"]) == len(offsets["Child"]) - 1:
                    child["Default"] = self.parse_param("string")
                    self.stream.read(8) # empty
                    child["Node Index"] = self.stream.read_u32() 
                else:
                    child["Condition Min"] = self.parse_param("float")
                    child["Condition Max"] = self.parse_param("float")
                    child["Node Index"] = self.stream.read_u32()
                entry["Child Nodes"].append(child)
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def BoneAnimation(self):
        entry = {}
        entry["Animation"] = self.parse_param("string")
        entry["Is Loop"] = self.parse_param("bool") # loop flag
        entry["Unknown 2"] = self.parse_param("bool") # for using the float
        entry["Unknown 3"] = self.parse_param("float") # frame offset for results?
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def InitialFrame(self):
        entry = {}
        # 1 = calc from norm current, 2 = calc from norm current complement
        # 6 = calc from current, 7 = calc from current complement
        # 8 = calc from norm single loop duration
        # 3 = calc include all loops
        # 4 = some bone comparison, returns 1.5x the duration
        # 5 = return provided end with no calc
        # else return start frame
        entry["Calc Mode"] = InitialFrameCalcMode(self.stream.read_u32()).name
        tag_offset = self.stream.read_u32()
        if tag_offset:
            pos = self.stream.tell()
            self.stream.seek(tag_offset)
            entry["Tags"] = self.read_tag_group()
            self.stream.seek(pos)
        entry["Unknown 1"] = self.parse_param("bool") # match tag or anim?
        entry["Bone 1"] = self.parse_param("string") # Used if flag is 4
        entry["Bone 2"] = self.parse_param("string") # Used if flag is 4
        # 0 = x, 1 = y, 2 = z
        # returns the start frame if bone 2 <= bone 1
        entry["Axis"] = Axis(self.stream.read_u32()).name # compare axis for bones
        entry["Calc Loop"] = self.parse_param("bool") # calc loop (skips the other checks if true)
        entry["Exclude Random Loops"] = self.parse_param("bool") # is not include random loop count
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def BoneBlender(self):
        entry = {}
        entry["Bone Group Name"] = self.parse_param("string")
        entry["Unknown 1"] = self.stream.read_u32() # 0 = first child node is the base anim, 1 = second is the base
        entry["Blend Rate"] = self.parse_param("float") # probably the blend rate
        # 2 = don't use value (use 1)
        # 4 = use value if rate < 0.5
        # 3 = use value if rate >= 0.5
        # 1 = use value
        # these operators are all inverted for the second bone
        # for other bones, 4 is becomes always use 1, 3 is use 1 if rate < 0.5, else use the value
        entry["Unknown 3"] = self.stream.read_u32() # comparison type for the value
        entry["Unknown 4"] = self.stream.read_u32() # some value (I think it's a bool)
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def BoolSelector(self):
        entry = {}
        entry["Parameter"] = self.parse_param("bool", True)
        entry["Is Sync"] = self.parse_param("bool") # is update sync frame
        entry["Force Run"] = bool(self.stream.read_u32()) # force select
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                if offsets["Child"].index(offset) == 0:
                    entry["Child Nodes"].append({"Condition True" : self.stream.read_u32()})
                else:
                    entry["Child Nodes"].append({"Condition False" : self.stream.read_u32()})
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    # I think this is for debugging purposes, I don't think these nodes are ever reached despite being present
    def Alert(self):
        entry = {}
        entry["Message"] = self.parse_param("string")
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def SubtractAnimation(self):
        entry = {}
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def ShapeAnimation(self):
        entry = {}
        entry["Animation"] = self.parse_param("string")
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry

    def Unknown7(self):
        entry = {}
        offsets, transitions, event, frame, state = self.read_connections()
        if offsets["Child"]:
            entry["Child Nodes"] = []
            for offset in offsets["Child"]:
                self.stream.seek(offset)
                entry["Child Nodes"].append(self.stream.read_u32())
        if state:
            entry["State Connections"] = state
        if transitions:
            entry["State Transitions"] = transitions
        if event:
            entry["Events"] = event
        if frame:
            entry["Frame Controls"] = frame
        return entry
    
    def import_baev(self, data):
        if isinstance(data, dict):
            events = BAEV.from_dict(data, self.filename).events
        else:
            events = BAEV.from_binary(data, self.filename).events
        for node in self.nodes:
            if node["Node Type"] == "Event":
                hash = "0x%08x" % calc_hash(node["GUID"])
                if hash in events:
                    node["BAEV Events"] = events[hash]
                else:
                    print(f"No BAEV event found for node index {node['Node Index']} (hash: {hash})")
        self.has_asnode_baev = True

    # what fun
    def write_parameter(self, buffer, value):
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
            flag = 1 << 0x1F
            if "Expression Index" in value:
                flag |= 1 << 0x18
                flag |= value["Expression Index"] & 0xFFFF
            elif "Calc Mode" in value:
                flag |= 1 << 0x1E
                flag |= self.current_calc_index & 0xFFFF
                self.current_calc_index += 1
            elif "Command Data Type" in value:
                flag |= 1 << 0x19
                flag |= value["Command Data Type"] & 0xFFFF
            elif "Blackboard Index" in value:
                flag |= value["Blackboard Index"] & 0xFFFF
                if "Axis" in value:
                    if value["Axis"] == "X":
                        flag |= 1 << 0x1A
                    elif value["Axis"] == "Y":
                        flag |= 2 << 0x1A
                    elif value["Axis"] == "Z":
                        flag |= 3 << 0x1A
                    else:
                        raise ValueError(f"Invalid Axis {value['Axis']}")
            else:
                raise ValueError("Could not determine parameter flags")
            if "Select Flag" in value:
                flag |= SelectFlag[value["Select Flag"]].value << 0x1C
            buffer.write(u32(flag))
            if "Input" in value:
                if type(value["Input"]) == int:
                    buffer.write(s32(value["Input"]))
                elif type(value["Input"]) == float:
                    buffer.write(f32(value["Input"]))
                elif type(value["Input"]) == str:
                    buffer.add_string(value["Input"])
                    buffer.write(u32(buffer._string_refs[value["Input"]]))
                elif type(value["Input"]) == bool:
                    buffer.write(u32(1 if value["Input"] else 0))
                elif type(value["Input"]) == list:
                    for v in value["Input"]:
                        buffer.write(f32(v))
                else:
                    raise ValueError(f"Invalid value {value['Input']}")
            elif "Default Value" in value and "Calc Mode" not in value:
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
                    raise ValueError(f"Invalid value {value['Default Value']}")
            else:
                buffer.write(u32(0))

    def to_json(self, output_dir=""):
        if output_dir:
            os.makedirs(output_dir, exist_ok=True)
        with open(os.path.join(output_dir, self.filename + ".json"), "w", encoding="utf-8") as f:
            json.dump(self.asdict(), f, indent=4, ensure_ascii=False)
    
     # Let's just do this all now so we don't have to jump back and fill in the offsets later
    def calc_offsets(self, body_sizes, event_count, sync_count, tag_groups, buffer):
        offsets = {}
        offset = 0x6C
        offset += 0x30 * len(self.commands)
        offset += 0x24 * len(self.nodes)
        offsets["Event Offsets"] = offset
        offset += 0x4 * event_count
        offsets["Node Bodies"] = offset
        for i in body_sizes:
            offset += body_sizes[i]
        offsets["Sync Indices"] = offset
        offset += 0x4 * sync_count
        offsets["Sync Control"] = offset
        for entry in self.sync_ctrl:
            offset += 0x18
            if "Fade In Frame" in entry:
                offset += 0xC
            elif "Sync Start Frame" in entry:
                offset += 0x18
        offsets["State Transitions"] = offset
        offset += 0x4 + 0x60 * len(self.state_transitions)
        offsets["Events"] = offset
        event_offsets = []
        for i, entry in enumerate(self.events):
            event_offsets.append(offset)
            offset += 0x8
            if "Trigger Events" in entry:
                for event in entry["Trigger Events"]:
                    offset += 0x18
                    offset += 0x4 + 0xc * len(event["Parameters"])
            if "Hold Events" in entry:
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
        offsets["Blackboard"] = offset
        offset += 0x30
        refs = []
        for datatype in self.blackboard:
            offset += 0x4 * len(self.blackboard[datatype])
            if datatype in ["int", "float", "bool", "string"]:
                offset += 0x4 * len(self.blackboard[datatype])
            elif datatype == "vec3f":
                offset += 0xc * len(self.blackboard[datatype])
            for param in self.blackboard[datatype]:
                if "Reference File" in param:
                    if param["Reference File"] not in refs:
                        refs.append(param["Reference File"])
        offset += 0x10 * len(refs)
        offsets["Partials"] = offset
        for entry in self.partials:
            offset += 0xc + 0x8 * len(entry["Bones"])
        offsets["Bone Groups"] = offset
        for entry in self.bone_groups:
            offset += 0x10 + 0x8 * len(entry["Bones"])
        offsets["Calc Control"] = offset
        offset += 0x20 * len(self.calc_ctrl)
        offsets["Tag List"] = offset
        offset += 4 + 4 * len(self.valid_tags)
        offsets["Tag Groups"] = offset
        tag_map = {}
        for entry in tag_groups:
            tag_map[tuple(entry)] = offset
            offset += 4 + 4 * len(entry)
        if self.expressions:
            offsets["EXB"] = offset
            pos = buffer.tell()
            exb = EXB(None, self.expressions, from_dict=True)
            offset = exb.ToBytes(exb, buffer, offsets["EXB"])
            buffer.seek(pos)
        else:
            offsets["EXB"] = 0
        offsets["ASMarkings"] = offset
        offset += 4 + 12 * len(self.as_markings)
        offsets["Material Blend"] = offset
        offset += 4 + 8 * len(self.material_blend)
        offsets["Enum"] = offset
        offset += 4
        offsets["Strings"] = offset
        return offsets, tag_map, event_offsets
    
    @staticmethod
    def calc_body_size(node, version):
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
            size = 0x84
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
            if "State Connections" in node["Body"]:
                size += 8 * len(node["Body"]["State Connections"]) # 4 for the offset and 4 for the index
            if "State Transitions" in node["Body"]:
                if version == 0x40F:
                    size += 8 * len(node["Body"]["State Transitions"]) # 4 for the offset and 4 for the index
                else:
                    size += 12 * len(node["Body"]["State Transitions"]) # 4 for the offset and 8 for the two indices
            if "Events" in node["Body"]:
                size += 8 * len(node["Body"]["Events"]) # 4 for the offset and 4 for the index
            if "Frame Controls" in node["Body"]:
                size += 8 * len(node["Body"]["Frame Controls"]) # 4 for the offset and 4 for the index
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

    def write_connections(self, buffer, node_body, node_type, tag_map={}):
        index = 0
        if "State Connections" in node_body:
            buffer.write(u8(len(node_body["State Connections"])))
            buffer.write(u8(index))
            index += len(node_body["State Connections"])
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
        if "State Transitions" in node_body:
            buffer.write(u8(len(node_body["State Transitions"])))
            buffer.write(u8(index))
            index += len(node_body["State Transitions"])
        else:
            buffer.write(u8(0))
            buffer.write(u8(index))
        if "Events" in node_body:
            buffer.write(u8(len(node_body["Events"])))
            buffer.write(u8(index))
            index += len(node_body["Events"])
        else:
            buffer.write(u8(0))
            buffer.write(u8(index))
        if "Frame Controls" in node_body:
            buffer.write(u8(len(node_body["Frame Controls"])))
            buffer.write(u8(index))
            index += len(node_body["Frame Controls"])
        else:
            buffer.write(u8(0))
            buffer.write(u8(index))
        offset = buffer.tell() + 4 * index
        if "State Connections" in node_body:
            for entry in node_body["State Connections"]:
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
        if "State Transitions" in node_body:
            for entry in node_body["State Transitions"]:
                buffer.write(u32(offset))
                offset += 8
        if "Events" in node_body:
            for entry in node_body["Events"]:
                buffer.write(u32(offset))
                offset += 4
        if "Frame Controls" in node_body:
            for entry in node_body["Frame Controls"]:
                buffer.write(u32(offset))
                offset += 4
        if "State Connections" in node_body:
            for entry in node_body["State Connections"]:
                buffer.write(u32(entry))
        if "Child Nodes" in node_body:
            for entry in node_body["Child Nodes"]:
                if node_type in ["BonePositionSelector", "FloatSelector", "OneDimensionalBlender"]:
                    if "Default" in entry:
                        self.write_parameter(buffer, entry["Default"])
                        buffer.write(u64(0))
                        buffer.write(u32(entry["Node Index"]))
                    else:
                        self.write_parameter(buffer, entry["Condition Min"])
                        self.write_parameter(buffer, entry["Condition Max"])
                        buffer.write(u32(entry["Node Index"]))
                elif node_type in ["RandomSelector", "IntSelector", "StringSelector"]:
                    # Note to self to handle enum resolultion for 0x40F
                    if "Weight" in entry:
                        self.write_parameter(buffer, entry["Weight"])
                    elif "Condition" in entry:
                        self.write_parameter(buffer, entry["Condition"])
                    elif "Default" in entry:
                        self.write_parameter(buffer, entry["Default"])
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
        if "State Transitions" in node_body:
            for entry in node_body["State Transitions"]:
                if entry["State Transition"]:
                    buffer.write(u32(self.state_transitions.index(entry["State Transition"])))
                else:
                    buffer.write(s32(-1))
                buffer.write(u32(entry["Node Index"]))
        if "Events" in node_body:
            for entry in node_body["Events"]:
                buffer.write(u32(entry))
        if "Frame Controls" in node_body:
            for entry in node_body["Frame Controls"]:
                buffer.write(u32(entry))

    @staticmethod
    def write_guid(buffer: WriteStream, guid):
        parts = guid.split("-")
        buffer.write(u32(int(parts[0], 16)))
        buffer.write(u16(int(parts[1], 16)))
        buffer.write(u16(int(parts[2], 16)))
        buffer.write(u8(int(parts[3][0:2], 16)))
        buffer.write(u8(int(parts[3][2:4], 16)))
        buffer.write(u8(int(parts[4][0:2], 16)))
        buffer.write(u8(int(parts[4][2:4], 16)))
        buffer.write(u8(int(parts[4][4:6], 16)))
        buffer.write(u8(int(parts[4][6:8], 16)))
        buffer.write(u8(int(parts[4][8:10], 16)))
        buffer.write(u8(int(parts[4][10:12], 16)))

    def to_binary(self, output_dir=""):
        if output_dir:
            os.makedirs(output_dir, exist_ok=True)
        self.current_calc_index = 0
        with open(os.path.join(output_dir, self.filename + ".asb"), "wb") as f:
            buffer = WriteStream(f)
            buffer.write("ASB ".encode())
            buffer.write(u32(self.version))
            buffer.add_string(self.filename)
            buffer.write(u32(buffer._string_refs[self.filename]))
            buffer.write(u32(len(self.commands)))
            buffer.write(u32(len(self.nodes)))
            buffer.write(u32(len(self.events)))
            buffer.write(u32(len(self.partials)))
            buffer.write(u32(len(self.sync_ctrl)))
            tag_groups = []
            body_sizes = {}
            for command in self.commands:
                if "Tags" in command:
                    if command["Tags"] not in tag_groups:
                        tag_groups.append(command["Tags"])
            event_count = 0
            sync_count = 0
            body_tags = []
            sync_ctrl = []
            for node in self.nodes:
                if "Tags" in node:
                    if node["Tags"] not in tag_groups:
                        tag_groups.append(node["Tags"])
                if "Sync Controls" in node:
                    for entry in node["Sync Controls"]:
                        sync_ctrl.append(entry)
                    sync_count += len(node["Sync Controls"])
                if node["Node Type"] == "PreviousTagSelector":
                    for child in node["Body"]["Child Nodes"]:
                        if child["Tags"] and child["Tags"] not in tag_groups:
                            body_tags.append(child["Tags"])
                if node["Node Type"] == "Event":
                    event_count += 1
                if node["Node Type"] == "InitialFrame":
                    if "Tags" in node["Body"] and node["Body"]["Tags"] not in tag_groups:
                        body_tags.append(node["Body"]["Tags"])
                body_sizes[node["Node Index"]] = self.calc_body_size(node, self.version)
            for tag in body_tags:
                if tag not in tag_groups:
                    tag_groups.append(tag)
            offsets, tag_map, event_offsets = self.calc_offsets(body_sizes, event_count, sync_count, tag_groups, buffer)
            buffer.write(u32(offsets["Blackboard"]))
            buffer.write(u32(offsets["Strings"]))
            buffer.write(u32(offsets["Enum"]))
            buffer.write(u32(offsets["State Transitions"]))
            buffer.write(u32(offsets["Event Offsets"]))
            buffer.write(u32(offsets["Partials"]))
            buffer.write(u32(offsets["Sync Control"]))
            buffer.write(u32(offsets["Sync Indices"]))
            buffer.write(u32(offsets["Calc Control"]))
            buffer.write(u32(len(self.calc_ctrl)))
            buffer.write(u32(offsets["Bone Groups"]))
            buffer.write(u32(len(self.bone_groups)))
            buffer.write(u32(0)) # string pool size to be written to later
            buffer.write(u32(offsets["Transitions"]))
            buffer.write(u32(offsets["Tag List"]))
            buffer.write(u32(offsets["ASMarkings"]))
            buffer.write(u32(offsets["EXB"]))
            buffer.write(u32(offsets["Command Groups"]))
            buffer.write(u32(offsets["Material Blend"]))
            for command in self.commands:
                buffer.add_string(command["Name"])
                buffer.write(u32(buffer._string_refs[command["Name"]]))
                if "Tags" in command:
                    for tag in command["Tags"]:
                        buffer.add_string(tag)
                    buffer.write(u32(tag_map[tuple(command["Tags"])]))
                else:
                    buffer.write(u32(0))
                self.write_parameter(buffer, command["Unknown 1"])
                self.write_parameter(buffer, command["Ignore Same Command"])
                buffer.write(u32(command["Interpolation Type"]))
                self.write_guid(buffer, command["GUID"])
                buffer.write(u16(command["Node Index"]))
                buffer.write(u16(0))
            body_offset = offsets["Node Bodies"]
            calc_index = 0
            sync_index = 0
            for node in self.nodes:
                buffer.write(u16(NodeType[node["Node Type"]].value))
                if "Sync Controls" in node:
                    buffer.write(u8(len(node["Sync Controls"])))
                else:
                    buffer.write(u8(0))
                buffer.write(u8(1 if node["No State Transition"] else 0))
                if "Tags" in node:
                    for tag in node["Tags"]:
                        buffer.add_string(tag)
                    buffer.write(u32(tag_map[tuple(node["Tags"])]))
                else:
                    buffer.write(u32(0))
                buffer.write(u32(body_offset))
                body_offset += body_sizes[node["Node Index"]]
                buffer.write(u16(calc_index))
                if "Calc Controllers" in node:
                    calc_index += len(node["Calc Controllers"])
                    buffer.write(u16(len(node["Calc Controllers"])))
                else:
                    buffer.write(u16(0))
                buffer.write(u16(sync_index))
                if "Sync Controls" in node:
                    sync_index += len(node["Sync Controls"])
                if "ASMarking" in node:
                    buffer.write(u16(self.as_markings.index(node["ASMarking"]) + 1))
                else:
                    buffer.write(u16(0))
                self.write_guid(buffer, node["GUID"])
            for offset in event_offsets:
                buffer.write(u32(offset))
            event_index = 0
            for node in self.nodes:
                if "Body" in node:
                    body = node["Body"]
                    if node["Node Type"] == "FloatSelector":
                        self.write_parameter(buffer, body["Parameter"])
                        self.write_parameter(buffer, body["Is Sync"])
                        buffer.write(u32(1 if body["Force Run"] else 0))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "StringSelector":
                        self.write_parameter(buffer, body["Parameter"])
                        self.write_parameter(buffer, body["Is Sync"])
                        buffer.write(u32(1 if body["Force Run"] else 0))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "SkeletalAnimation":
                        self.write_parameter(buffer, body["Animation"])
                        buffer.write(u32(body["Unknown 1"]))
                        buffer.write(u32(body["Unknown 2"]))
                        self.write_parameter(buffer, body["Unknown 3"])
                        self.write_parameter(buffer, body["Unknown 4"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "State":
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "OneDimensionalBlender":
                        self.write_parameter(buffer, body["Parameter"])
                        buffer.write(u32(body["Lerp Mode"]))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "Sequential":
                        self.write_parameter(buffer, body["Use Sync Range Mult"])
                        self.write_parameter(buffer, body["Sync Range Mult"])
                        self.write_parameter(buffer, body["Unknown 3"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "IntSelector":
                        self.write_parameter(buffer, body["Parameter"])
                        self.write_parameter(buffer, body["Is Sync"])
                        buffer.write(u32(1 if body["Force Run"] else 0))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "Simultaneous":
                        buffer.write(u32(body["Finish With Child"]))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "Event":
                        buffer.write(u32(event_index))
                        event_index += 1
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "MaterialAnimation":
                        if "Material Blend Setting" in body:
                            buffer.write(u32(self.material_blend.index(body["Material Blend Setting"]) + 1))
                        else:
                            buffer.write(u32(0))
                        self.write_parameter(buffer, body["Animation"])
                        self.write_parameter(buffer, body["Is Loop"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "FrameController":
                        self.write_parameter(buffer, body["Animation Rate"])
                        self.write_parameter(buffer, body["Start Frame"])
                        self.write_parameter(buffer, body["End Frame"])
                        buffer.write(u32(body["Loop Flags"]))
                        self.write_parameter(buffer, body["Loop Cancel Flag"])
                        self.write_parameter(buffer, body["Unknown 2"])
                        self.write_parameter(buffer, body["Loop Num"])
                        self.write_parameter(buffer, body["Max Random Loop Num"])
                        self.write_parameter(buffer, body["Is Not Use Random Bonus Loop"])
                        self.write_parameter(buffer, body["Animation Freeze Point"])
                        self.write_parameter(buffer, body["Animation Freeze Frame"])
                        self.write_parameter(buffer, body["Loop Duration"])
                        buffer.write(u32(1 if body["Is Include Initial Loop"] else 0))
                        self.write_parameter(buffer, body["Unknown 10"])
                        self.write_parameter(buffer, body["Unknown 11"])
                        buffer.write(u32(body["Unknown 12"]))
                        buffer.write(u32(body["Unknown 13"]))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "DummyAnimation":
                        self.write_parameter(buffer, body["Frame"])
                        self.write_parameter(buffer, body["Is Loop"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "RandomSelector":
                        buffer.write(u32(SelectFlag[body["Select Flag"]].value))
                        self.write_parameter(buffer, body["Is Sync"])
                        self.write_parameter(buffer, body["Max Cached Select Count"])
                        buffer.write(u32(1 if body["Force Run"] else 0))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "PreviousTagSelector":
                        buffer.write(u32(body["Tag Set Index"]))
                        self.write_connections(buffer, body, node["Node Type"], tag_map)
                    elif node["Node Type"] == "BonePositionSelector":
                        self.write_parameter(buffer, body["Bone 1"])
                        self.write_parameter(buffer, body["Bone 2"])
                        buffer.write(u32(Axis[body["Axis"]].value))
                        buffer.write(u32(SelectFlag[body["Select Flag"]].value))
                        self.write_parameter(buffer, body["Is Sync"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "BoneAnimation":
                        self.write_parameter(buffer, body["Animation"])
                        self.write_parameter(buffer, body["Is Loop"])
                        self.write_parameter(buffer, body["Unknown 2"])
                        self.write_parameter(buffer, body["Unknown 3"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "InitialFrame":
                        buffer.write(u32(InitialFrameCalcMode[body["Calc Mode"]].value))
                        if "Tags" in body:
                            buffer.write(u32(tag_map[tuple(body["Tags"])]))
                        else:
                            buffer.write(u32(0))
                        self.write_parameter(buffer, body["Unknown 1"])
                        self.write_parameter(buffer, body["Bone 1"])
                        self.write_parameter(buffer, body["Bone 2"])
                        buffer.write(u32(Axis[body["Axis"]].value))
                        self.write_parameter(buffer, body["Calc Loop"])
                        self.write_parameter(buffer, body["Exclude Random Loops"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "BoneBlender":
                        self.write_parameter(buffer, body["Bone Group Name"])
                        buffer.write(u32(body["Unknown 1"]))
                        self.write_parameter(buffer, body["Blend Rate"])
                        buffer.write(u32(body["Unknown 3"]))
                        buffer.write(u32(body["Unknown 4"]))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "BoolSelector":
                        self.write_parameter(buffer, body["Parameter"])
                        self.write_parameter(buffer, body["Is Sync"])
                        buffer.write(u32(1 if body["Force Run"] else 0))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "Alert":
                        self.write_parameter(buffer, body["Message"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "SubtractAnimation":
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "ShapeAnimation":
                        self.write_parameter(buffer, body["Animation"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "Unknown7":
                        self.write_connections(buffer, body, node["Node Type"])
                    else:
                        raise ValueError(f"Invalid node type {node['Node Type']}")
            for entry in sync_ctrl:
                buffer.write(u32(self.sync_ctrl.index(entry)))
            value_offset = offsets["Sync Control"] + 0x18 * len(self.sync_ctrl)
            for entry in self.sync_ctrl:
                if "Fade In Frame" in entry:
                    type = 0
                elif "Sync Start Frame" in entry:
                    type = 1
                else:
                    type = 3
                buffer.write(u32(type))
                buffer.write(u32(value_offset))
                if type == 0:
                    value_offset += 12
                elif type == 1:
                    value_offset += 24
                # Scuffed but works
                self.write_guid(buffer, entry["GUID"])
            for entry in self.sync_ctrl:
                if "Fade In Frame" in entry:
                    self.write_parameter(buffer, entry["Fade In Frame"])
                    buffer.write(u32(entry["Unknown"]))
                elif "Sync Start Frame" in entry:
                    self.write_parameter(buffer, entry["Sync Start Frame"])
                    self.write_parameter(buffer, entry["Normalized Sync Start Frame"])
                    self.write_parameter(buffer, entry["Unknown"])
            buffer.write(u32(len(self.state_transitions)))
            for entry in self.state_transitions:
                buffer.write(u16(entry["Current Node"]))
                buffer.write(u16(entry["Target Node"]))
                buffer.write(u32(StateCheckType[entry["Check Type"]].value))
                buffer.write(u8(1 if entry["Transition to Next Instead of Target"] else 0))
                buffer.write(u8(1 if entry["Skip Transition"] else 0))
                buffer.write(u16(0))
                buffer.write(u32(entry["Unknown"]))
                for param in entry["Parameters"]:
                    if "Value 1" in param:
                        if isinstance(param["Value 1"], dict):
                            if "Type" in param["Value 1"]:
                                if param["Value 1"]["Type"] in ["float", "vec3f"]:
                                    buffer.write(u16(1))
                                elif param["Value 1"]["Type"] == "int":
                                    buffer.write(u16(2))
                                elif param["Value 1"]["Type"] == "string":
                                    buffer.write(u16(3))
                                else:
                                    raise ValueError("Invalid State Transition Parameter Type")
                            elif "Command Data Type" in param["Value 1"]:
                                if param["Value 1"]["Command Data Type"] == 3:
                                    buffer.write(u16(3))
                                else:
                                    buffer.write(u16(1))
                            elif "Input" in param["Value 1"]:
                                if isinstance(param["Value 1"]["Input"], float):
                                    buffer.write(u16(1))
                                elif isinstance(param["Value 1"]["Input"], int):
                                    buffer.write(u16(2))
                                elif isinstance(param["Value 1"]["Input"], str):
                                    buffer.write(u16(3))
                                else:
                                    raise ValueError("Invalid State Transition Parameter Type")
                        elif isinstance(param["Value 1"], float):
                            buffer.write(u16(1))
                        elif isinstance(param["Value 1"], int):
                            buffer.write(u16(2))
                        elif isinstance(param["Value 1"], str):
                            buffer.write(u16(3))
                        else:
                            raise ValueError("Invalid State Transition Parameter Type")
                    else:
                        buffer.write(u16(0))
                    buffer.write(u16(CompareOperator[param["Compare Type"]].value))
                    if "Value 1" in param:
                        self.write_parameter(buffer, param["Value 1"])
                        self.write_parameter(buffer, param["Value 2"])
                    else:
                        buffer.write(u64(0))
                        buffer.write(u64(0))
            for event in self.events:
                buffer.write(u32(len(event["Trigger Events"]) if "Trigger Events" in event else 0))
                buffer.write(u32(len(event["Hold Events"]) if "Hold Events" in event else 0))
                offset = buffer.tell() + 0x18 * (len(event["Trigger Events"]) if "Trigger Events" in event else 0)\
                    + 0x1c * (len(event["Hold Events"]) if "Hold Events" in event else 0)
                if "Trigger Events" in event:
                    for trigger in event["Trigger Events"]:
                        buffer.add_string(trigger["Name"])
                        buffer.write(u32(buffer._string_refs[trigger["Name"]]))
                        buffer.write(u32(trigger["Unknown"]))
                        buffer.write(u32(offset))
                        offset += 4 + 4 * len(trigger["Parameters"])
                        buffer.write(u32(8 * len(trigger["Parameters"])))
                        buffer.write(u32(int(trigger["Hash"], 16)))
                        buffer.write(f32(trigger["Start Frame"]))
                if "Hold Events" in event:
                    for hold in event["Hold Events"]:
                        buffer.add_string(hold["Name"])
                        buffer.write(u32(buffer._string_refs[hold["Name"]]))
                        buffer.write(u32(hold["Unknown"]))
                        buffer.write(u32(offset))
                        offset += 4 + 4 * len(hold["Parameters"])
                        buffer.write(u32(8 * len(hold["Parameters"])))
                        buffer.write(u32(int(hold["Hash"], 16)))
                        buffer.write(f32(hold["Start Frame"]))
                        buffer.write(f32(hold["End Frame"]))
                if "Trigger Events" in event:
                    for trigger in event["Trigger Events"]:
                        buffer.write(u32(len(trigger["Parameters"])))
                        for param in trigger["Parameters"]:
                            if isinstance(param, str):
                                flag = 0x40 << 24
                            elif isinstance(param, float):
                                flag = 0x30 << 24
                            elif isinstance(param, bool): # have to put bool first bc bool inherits from int
                                flag = 0x10 << 24
                            elif isinstance(param, int):
                                flag = 0x20 << 24
                            else:
                                raise ValueError(param)
                            buffer.write(u32(offset | flag))
                            offset += 8
                if "Hold Events" in event:
                    for hold in event["Hold Events"]:
                        buffer.write(u32(len(hold["Parameters"])))
                        for param in hold["Parameters"]:
                            if isinstance(param, str):
                                flag = 0x40 << 24
                            elif isinstance(param, float):
                                flag = 0x30 << 24
                            elif isinstance(param, bool):
                                flag = 0x10 << 24
                            elif isinstance(param, int):
                                flag = 0x20 << 24
                            else:
                                raise ValueError(param)
                            buffer.write(u32(offset | flag))
                            offset += 8
                if "Trigger Events" in event:
                    for trigger in event["Trigger Events"]:
                        for param in trigger["Parameters"]:
                            self.write_parameter(buffer, param)
                if "Hold Events" in event:
                    for hold in event["Hold Events"]:
                        for param in hold["Parameters"]:
                            self.write_parameter(buffer, param)
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
                    buffer.add_string(entry["Current Command"])
                    buffer.write(u32(buffer._string_refs[entry["Current Command"]]))
                    buffer.add_string(entry["Next Command"])
                    buffer.write(u32(buffer._string_refs[entry["Next Command"]]))
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
                    self.write_parameter(buffer, entry["Value"])
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
            if self.blackboard:
                index = 0
                pos = 0
                for t in blackboard_types:
                    if t in self.blackboard:
                        buffer.write(u16(len(self.blackboard[t])))
                    else:
                        buffer.write(u16(0))
                    buffer.write(u16(index))
                    if t in self.blackboard:
                        index += len(self.blackboard[t])
                    if t == "vec3f" and "vec3f" in self.blackboard:
                        buffer.write(u16(pos))
                        pos = pos + len(self.blackboard[t]) * 12
                    elif t in self.blackboard:
                        buffer.write(u16(pos))
                        pos = pos + len(self.blackboard[t]) * 4
                    else:
                        buffer.write(u16(pos))
                    buffer.write(u16(0))
                files = []
                for t in self.blackboard:
                    for entry in self.blackboard[t]:
                        buffer.add_string(entry["Name"])
                        name_offset = buffer._string_refs[entry["Name"]]
                        if "Reference File" in entry:
                            if entry["Reference File"] not in files:
                                files.append(entry["Reference File"])
                            name_offset = name_offset | (1 << 31)
                            name_offset = name_offset | (files.index(entry["Reference File"]) << 24)
                        buffer.write(u32(name_offset))
                start = buffer.tell()
                size = 0
                for t in self.blackboard:
                    for entry in self.blackboard[t]:
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
                    buffer.add_string(file)
                    buffer.write(u32(buffer._string_refs[file]))
                    buffer.write(u32(mmh3.hash(file, signed=False)))
                    buffer.write(u32(mmh3.hash(os.path.splitext(os.path.basename(file))[0], signed=False)))
                    buffer.write(u32(mmh3.hash(os.path.splitext(file)[1].replace('.', ''), signed=False)))        
            else:
                buffer.skip(48)
            for entry in self.partials:
                buffer.write(u16(len(entry["Bones"])))
                buffer.write(u16(1 if entry["Is Material Slot"] else 0))
                buffer.add_string(entry["Name"])
                buffer.add_string(entry["Unknown"])
                buffer.write(u32(buffer._string_refs[entry["Name"]]))
                buffer.write(u32(buffer._string_refs[entry["Unknown"]]))
                for slot in entry["Bones"]:
                    buffer.add_string(slot["Name"])
                    buffer.write(u32(buffer._string_refs[slot["Name"]]))
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
            for entry in self.calc_ctrl:
                flag = 0
                if "Command Data Type" in entry["Parameter"]:
                    flag |= 1 << 0x1F
                    flag |= entry["Parameter"]["Command Data Type"] & 0xFFFF
                else:
                    flag |= entry["Parameter"]["Blackboard Index"] & 0xFFFF
                buffer.write(u32(flag))
                buffer.write(f32(entry["Adjust Value"]))
                buffer.write(u32(Mode[entry["Calc Mode"]].value))
                buffer.write(f32(entry["Default Value"]))
                buffer.write(f32(entry["Adjust Rate"]))
                buffer.write(f32(entry["Base Result"]))
                buffer.write(f32(entry["Min"]))
                buffer.write(f32(entry["Max"]))
            buffer.write(u32(len(self.valid_tags)))
            for tag in self.valid_tags:
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
            buffer.write(u32(len(self.material_blend)))
            for entry in self.material_blend:
                buffer.add_string(entry["Name"])
                buffer.write(u32(buffer._string_refs[entry["Name"]]))
                buffer.write(f32(entry["Blend Start"]))
            buffer.write(u32(0)) # enum resolve
            buffer.write(buffer._strings)
            buffer.seek(0x50)
            buffer.write(u32(len(buffer._strings)))
        
        if self.has_asnode_baev:
            events = {}
            for node in self.nodes:
                if "BAEV Events" in node:
                    events["0x%08x" % calc_hash(node["GUID"])] = node["BAEV Events"]
            anim_events = BAEV.from_dict(events, self.filename)
            anim_events.to_binary(output_dir)
            
            
    def to_binary_stream(self):
        self.current_calc_index = 0
        result_data_asb = None
        with io.BytesIO() as f:
            buffer = WriteStream(f)
            buffer.write("ASB ".encode())
            buffer.write(u32(self.version))
            buffer.add_string(self.filename)
            buffer.write(u32(buffer._string_refs[self.filename]))
            buffer.write(u32(len(self.commands)))
            buffer.write(u32(len(self.nodes)))
            buffer.write(u32(len(self.events)))
            buffer.write(u32(len(self.partials)))
            buffer.write(u32(len(self.sync_ctrl)))
            tag_groups = []
            body_sizes = {}
            for command in self.commands:
                if "Tags" in command:
                    if command["Tags"] not in tag_groups:
                        tag_groups.append(command["Tags"])
            event_count = 0
            sync_count = 0
            body_tags = []
            sync_ctrl = []
            for node in self.nodes:
                if "Tags" in node:
                    if node["Tags"] not in tag_groups:
                        tag_groups.append(node["Tags"])
                if "Sync Controls" in node:
                    for entry in node["Sync Controls"]:
                        sync_ctrl.append(entry)
                    sync_count += len(node["Sync Controls"])
                if node["Node Type"] == "PreviousTagSelector":
                    for child in node["Body"]["Child Nodes"]:
                        if child["Tags"] and child["Tags"] not in tag_groups:
                            body_tags.append(child["Tags"])
                if node["Node Type"] == "Event":
                    event_count += 1
                if node["Node Type"] == "InitialFrame":
                    if "Tags" in node["Body"] and node["Body"]["Tags"] not in tag_groups:
                        body_tags.append(node["Body"]["Tags"])
                body_sizes[node["Node Index"]] = self.calc_body_size(node, self.version)
            for tag in body_tags:
                if tag not in tag_groups:
                    tag_groups.append(tag)
            offsets, tag_map, event_offsets = self.calc_offsets(body_sizes, event_count, sync_count, tag_groups, buffer)
            buffer.write(u32(offsets["Blackboard"]))
            buffer.write(u32(offsets["Strings"]))
            buffer.write(u32(offsets["Enum"]))
            buffer.write(u32(offsets["State Transitions"]))
            buffer.write(u32(offsets["Event Offsets"]))
            buffer.write(u32(offsets["Partials"]))
            buffer.write(u32(offsets["Sync Control"]))
            buffer.write(u32(offsets["Sync Indices"]))
            buffer.write(u32(offsets["Calc Control"]))
            buffer.write(u32(len(self.calc_ctrl)))
            buffer.write(u32(offsets["Bone Groups"]))
            buffer.write(u32(len(self.bone_groups)))
            buffer.write(u32(0)) # string pool size to be written to later
            buffer.write(u32(offsets["Transitions"]))
            buffer.write(u32(offsets["Tag List"]))
            buffer.write(u32(offsets["ASMarkings"]))
            buffer.write(u32(offsets["EXB"]))
            buffer.write(u32(offsets["Command Groups"]))
            buffer.write(u32(offsets["Material Blend"]))
            for command in self.commands:
                buffer.add_string(command["Name"])
                buffer.write(u32(buffer._string_refs[command["Name"]]))
                if "Tags" in command:
                    for tag in command["Tags"]:
                        buffer.add_string(tag)
                    buffer.write(u32(tag_map[tuple(command["Tags"])]))
                else:
                    buffer.write(u32(0))
                self.write_parameter(buffer, command["Unknown 1"])
                self.write_parameter(buffer, command["Ignore Same Command"])
                buffer.write(u32(command["Interpolation Type"]))
                self.write_guid(buffer, command["GUID"])
                buffer.write(u16(command["Node Index"]))
                buffer.write(u16(0))
            body_offset = offsets["Node Bodies"]
            calc_index = 0
            sync_index = 0
            for node in self.nodes:
                buffer.write(u16(NodeType[node["Node Type"]].value))
                if "Sync Controls" in node:
                    buffer.write(u8(len(node["Sync Controls"])))
                else:
                    buffer.write(u8(0))
                buffer.write(u8(1 if node["No State Transition"] else 0))
                if "Tags" in node:
                    for tag in node["Tags"]:
                        buffer.add_string(tag)
                    buffer.write(u32(tag_map[tuple(node["Tags"])]))
                else:
                    buffer.write(u32(0))
                buffer.write(u32(body_offset))
                body_offset += body_sizes[node["Node Index"]]
                buffer.write(u16(calc_index))
                if "Calc Controllers" in node:
                    calc_index += len(node["Calc Controllers"])
                    buffer.write(u16(len(node["Calc Controllers"])))
                else:
                    buffer.write(u16(0))
                buffer.write(u16(sync_index))
                if "Sync Controls" in node:
                    sync_index += len(node["Sync Controls"])
                if "ASMarking" in node:
                    buffer.write(u16(self.as_markings.index(node["ASMarking"]) + 1))
                else:
                    buffer.write(u16(0))
                self.write_guid(buffer, node["GUID"])
            for offset in event_offsets:
                buffer.write(u32(offset))
            event_index = 0
            for node in self.nodes:
                if "Body" in node:
                    body = node["Body"]
                    if node["Node Type"] == "FloatSelector":
                        self.write_parameter(buffer, body["Parameter"])
                        self.write_parameter(buffer, body["Is Sync"])
                        buffer.write(u32(1 if body["Force Run"] else 0))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "StringSelector":
                        self.write_parameter(buffer, body["Parameter"])
                        self.write_parameter(buffer, body["Is Sync"])
                        buffer.write(u32(1 if body["Force Run"] else 0))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "SkeletalAnimation":
                        self.write_parameter(buffer, body["Animation"])
                        buffer.write(u32(body["Unknown 1"]))
                        buffer.write(u32(body["Unknown 2"]))
                        self.write_parameter(buffer, body["Unknown 3"])
                        self.write_parameter(buffer, body["Unknown 4"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "State":
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "OneDimensionalBlender":
                        self.write_parameter(buffer, body["Parameter"])
                        buffer.write(u32(body["Lerp Mode"]))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "Sequential":
                        self.write_parameter(buffer, body["Use Sync Range Mult"])
                        self.write_parameter(buffer, body["Sync Range Mult"])
                        self.write_parameter(buffer, body["Unknown 3"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "IntSelector":
                        self.write_parameter(buffer, body["Parameter"])
                        self.write_parameter(buffer, body["Is Sync"])
                        buffer.write(u32(1 if body["Force Run"] else 0))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "Simultaneous":
                        buffer.write(u32(body["Finish With Child"]))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "Event":
                        buffer.write(u32(event_index))
                        event_index += 1
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "MaterialAnimation":
                        if "Material Blend Setting" in body:
                            buffer.write(u32(self.material_blend.index(body["Material Blend Setting"]) + 1))
                        else:
                            buffer.write(u32(0))
                        self.write_parameter(buffer, body["Animation"])
                        self.write_parameter(buffer, body["Is Loop"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "FrameController":
                        self.write_parameter(buffer, body["Animation Rate"])
                        self.write_parameter(buffer, body["Start Frame"])
                        self.write_parameter(buffer, body["End Frame"])
                        buffer.write(u32(body["Loop Flags"]))
                        self.write_parameter(buffer, body["Loop Cancel Flag"])
                        self.write_parameter(buffer, body["Unknown 2"])
                        self.write_parameter(buffer, body["Loop Num"])
                        self.write_parameter(buffer, body["Max Random Loop Num"])
                        self.write_parameter(buffer, body["Is Not Use Random Bonus Loop"])
                        self.write_parameter(buffer, body["Animation Freeze Point"])
                        self.write_parameter(buffer, body["Animation Freeze Frame"])
                        self.write_parameter(buffer, body["Loop Duration"])
                        buffer.write(u32(1 if body["Is Include Initial Loop"] else 0))
                        self.write_parameter(buffer, body["Unknown 10"])
                        self.write_parameter(buffer, body["Unknown 11"])
                        buffer.write(u32(body["Unknown 12"]))
                        buffer.write(u32(body["Unknown 13"]))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "DummyAnimation":
                        self.write_parameter(buffer, body["Frame"])
                        self.write_parameter(buffer, body["Is Loop"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "RandomSelector":
                        buffer.write(u32(SelectFlag[body["Select Flag"]].value))
                        self.write_parameter(buffer, body["Is Sync"])
                        self.write_parameter(buffer, body["Max Cached Select Count"])
                        buffer.write(u32(1 if body["Force Run"] else 0))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "PreviousTagSelector":
                        buffer.write(u32(body["Tag Set Index"]))
                        self.write_connections(buffer, body, node["Node Type"], tag_map)
                    elif node["Node Type"] == "BonePositionSelector":
                        self.write_parameter(buffer, body["Bone 1"])
                        self.write_parameter(buffer, body["Bone 2"])
                        buffer.write(u32(Axis[body["Axis"]].value))
                        buffer.write(u32(SelectFlag[body["Select Flag"]].value))
                        self.write_parameter(buffer, body["Is Sync"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "BoneAnimation":
                        self.write_parameter(buffer, body["Animation"])
                        self.write_parameter(buffer, body["Is Loop"])
                        self.write_parameter(buffer, body["Unknown 2"])
                        self.write_parameter(buffer, body["Unknown 3"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "InitialFrame":
                        buffer.write(u32(InitialFrameCalcMode[body["Calc Mode"]].value))
                        if "Tags" in body:
                            buffer.write(u32(tag_map[tuple(body["Tags"])]))
                        else:
                            buffer.write(u32(0))
                        self.write_parameter(buffer, body["Unknown 1"])
                        self.write_parameter(buffer, body["Bone 1"])
                        self.write_parameter(buffer, body["Bone 2"])
                        buffer.write(u32(Axis[body["Axis"]].value))
                        self.write_parameter(buffer, body["Calc Loop"])
                        self.write_parameter(buffer, body["Exclude Random Loops"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "BoneBlender":
                        self.write_parameter(buffer, body["Bone Group Name"])
                        buffer.write(u32(body["Unknown 1"]))
                        self.write_parameter(buffer, body["Blend Rate"])
                        buffer.write(u32(body["Unknown 3"]))
                        buffer.write(u32(body["Unknown 4"]))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "BoolSelector":
                        self.write_parameter(buffer, body["Parameter"])
                        self.write_parameter(buffer, body["Is Sync"])
                        buffer.write(u32(1 if body["Force Run"] else 0))
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "Alert":
                        self.write_parameter(buffer, body["Message"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "SubtractAnimation":
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "ShapeAnimation":
                        self.write_parameter(buffer, body["Animation"])
                        self.write_connections(buffer, body, node["Node Type"])
                    elif node["Node Type"] == "Unknown7":
                        self.write_connections(buffer, body, node["Node Type"])
                    else:
                        raise ValueError(f"Invalid node type {node['Node Type']}")
            for entry in sync_ctrl:
                buffer.write(u32(self.sync_ctrl.index(entry)))
            value_offset = offsets["Sync Control"] + 0x18 * len(self.sync_ctrl)
            for entry in self.sync_ctrl:
                if "Fade In Frame" in entry:
                    type = 0
                elif "Sync Start Frame" in entry:
                    type = 1
                else:
                    type = 3
                buffer.write(u32(type))
                buffer.write(u32(value_offset))
                if type == 0:
                    value_offset += 12
                elif type == 1:
                    value_offset += 24
                # Scuffed but works
                self.write_guid(buffer, entry["GUID"])
            for entry in self.sync_ctrl:
                if "Fade In Frame" in entry:
                    self.write_parameter(buffer, entry["Fade In Frame"])
                    buffer.write(u32(entry["Unknown"]))
                elif "Sync Start Frame" in entry:
                    self.write_parameter(buffer, entry["Sync Start Frame"])
                    self.write_parameter(buffer, entry["Normalized Sync Start Frame"])
                    self.write_parameter(buffer, entry["Unknown"])
            buffer.write(u32(len(self.state_transitions)))
            for entry in self.state_transitions:
                buffer.write(u16(entry["Current Node"]))
                buffer.write(u16(entry["Target Node"]))
                buffer.write(u32(StateCheckType[entry["Check Type"]].value))
                buffer.write(u8(1 if entry["Transition to Next Instead of Target"] else 0))
                buffer.write(u8(1 if entry["Skip Transition"] else 0))
                buffer.write(u16(0))
                buffer.write(u32(entry["Unknown"]))
                for param in entry["Parameters"]:
                    if "Value 1" in param:
                        if isinstance(param["Value 1"], dict):
                            if "Type" in param["Value 1"]:
                                if param["Value 1"]["Type"] in ["float", "vec3f"]:
                                    buffer.write(u16(1))
                                elif param["Value 1"]["Type"] == "int":
                                    buffer.write(u16(2))
                                elif param["Value 1"]["Type"] == "string":
                                    buffer.write(u16(3))
                                else:
                                    raise ValueError("Invalid State Transition Parameter Type")
                            elif "Command Data Type" in param["Value 1"]:
                                if param["Value 1"]["Command Data Type"] == 3:
                                    buffer.write(u16(3))
                                else:
                                    buffer.write(u16(1))
                            elif "Input" in param["Value 1"]:
                                if isinstance(param["Value 1"]["Input"], float):
                                    buffer.write(u16(1))
                                elif isinstance(param["Value 1"]["Input"], int):
                                    buffer.write(u16(2))
                                elif isinstance(param["Value 1"]["Input"], str):
                                    buffer.write(u16(3))
                                else:
                                    raise ValueError("Invalid State Transition Parameter Type")
                        elif isinstance(param["Value 1"], float):
                            buffer.write(u16(1))
                        elif isinstance(param["Value 1"], int):
                            buffer.write(u16(2))
                        elif isinstance(param["Value 1"], str):
                            buffer.write(u16(3))
                        else:
                            raise ValueError("Invalid State Transition Parameter Type")
                    else:
                        buffer.write(u16(0))
                    buffer.write(u16(CompareOperator[param["Compare Type"]].value))
                    if "Value 1" in param:
                        self.write_parameter(buffer, param["Value 1"])
                        self.write_parameter(buffer, param["Value 2"])
                    else:
                        buffer.write(u64(0))
                        buffer.write(u64(0))
            for event in self.events:
                buffer.write(u32(len(event["Trigger Events"]) if "Trigger Events" in event else 0))
                buffer.write(u32(len(event["Hold Events"]) if "Hold Events" in event else 0))
                offset = buffer.tell() + 0x18 * (len(event["Trigger Events"]) if "Trigger Events" in event else 0)\
                    + 0x1c * (len(event["Hold Events"]) if "Hold Events" in event else 0)
                if "Trigger Events" in event:
                    for trigger in event["Trigger Events"]:
                        buffer.add_string(trigger["Name"])
                        buffer.write(u32(buffer._string_refs[trigger["Name"]]))
                        buffer.write(u32(trigger["Unknown"]))
                        buffer.write(u32(offset))
                        offset += 4 + 4 * len(trigger["Parameters"])
                        buffer.write(u32(8 * len(trigger["Parameters"])))
                        buffer.write(u32(int(trigger["Hash"], 16)))
                        buffer.write(f32(trigger["Start Frame"]))
                if "Hold Events" in event:
                    for hold in event["Hold Events"]:
                        buffer.add_string(hold["Name"])
                        buffer.write(u32(buffer._string_refs[hold["Name"]]))
                        buffer.write(u32(hold["Unknown"]))
                        buffer.write(u32(offset))
                        offset += 4 + 4 * len(hold["Parameters"])
                        buffer.write(u32(8 * len(hold["Parameters"])))
                        buffer.write(u32(int(hold["Hash"], 16)))
                        buffer.write(f32(hold["Start Frame"]))
                        buffer.write(f32(hold["End Frame"]))
                if "Trigger Events" in event:
                    for trigger in event["Trigger Events"]:
                        buffer.write(u32(len(trigger["Parameters"])))
                        for param in trigger["Parameters"]:
                            if isinstance(param, str):
                                flag = 0x40 << 24
                            elif isinstance(param, float):
                                flag = 0x30 << 24
                            elif isinstance(param, bool): # have to put bool first bc bool inherits from int
                                flag = 0x10 << 24
                            elif isinstance(param, int):
                                flag = 0x20 << 24
                            else:
                                raise ValueError(param)
                            buffer.write(u32(offset | flag))
                            offset += 8
                if "Hold Events" in event:
                    for hold in event["Hold Events"]:
                        buffer.write(u32(len(hold["Parameters"])))
                        for param in hold["Parameters"]:
                            if isinstance(param, str):
                                flag = 0x40 << 24
                            elif isinstance(param, float):
                                flag = 0x30 << 24
                            elif isinstance(param, bool):
                                flag = 0x10 << 24
                            elif isinstance(param, int):
                                flag = 0x20 << 24
                            else:
                                raise ValueError(param)
                            buffer.write(u32(offset | flag))
                            offset += 8
                if "Trigger Events" in event:
                    for trigger in event["Trigger Events"]:
                        for param in trigger["Parameters"]:
                            self.write_parameter(buffer, param)
                if "Hold Events" in event:
                    for hold in event["Hold Events"]:
                        for param in hold["Parameters"]:
                            self.write_parameter(buffer, param)
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
                    buffer.add_string(entry["Current Command"])
                    buffer.write(u32(buffer._string_refs[entry["Current Command"]]))
                    buffer.add_string(entry["Next Command"])
                    buffer.write(u32(buffer._string_refs[entry["Next Command"]]))
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
                    self.write_parameter(buffer, entry["Value"])
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
            if self.blackboard:
                index = 0
                pos = 0
                for t in blackboard_types:
                    if t in self.blackboard:
                        buffer.write(u16(len(self.blackboard[t])))
                    else:
                        buffer.write(u16(0))
                    buffer.write(u16(index))
                    if t in self.blackboard:
                        index += len(self.blackboard[t])
                    if t == "vec3f" and "vec3f" in self.blackboard:
                        buffer.write(u16(pos))
                        pos = pos + len(self.blackboard[t]) * 12
                    elif t in self.blackboard:
                        buffer.write(u16(pos))
                        pos = pos + len(self.blackboard[t]) * 4
                    else:
                        buffer.write(u16(pos))
                    buffer.write(u16(0))
                files = []
                for t in self.blackboard:
                    for entry in self.blackboard[t]:
                        buffer.add_string(entry["Name"])
                        name_offset = buffer._string_refs[entry["Name"]]
                        if "Reference File" in entry:
                            if entry["Reference File"] not in files:
                                files.append(entry["Reference File"])
                            name_offset = name_offset | (1 << 31)
                            name_offset = name_offset | (files.index(entry["Reference File"]) << 24)
                        buffer.write(u32(name_offset))
                start = buffer.tell()
                size = 0
                for t in self.blackboard:
                    for entry in self.blackboard[t]:
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
                    buffer.add_string(file)
                    buffer.write(u32(buffer._string_refs[file]))
                    buffer.write(u32(mmh3.hash(file, signed=False)))
                    buffer.write(u32(mmh3.hash(os.path.splitext(os.path.basename(file))[0], signed=False)))
                    buffer.write(u32(mmh3.hash(os.path.splitext(file)[1].replace('.', ''), signed=False)))        
            else:
                buffer.skip(48)
            for entry in self.partials:
                buffer.write(u16(len(entry["Bones"])))
                buffer.write(u16(1 if entry["Is Material Slot"] else 0))
                buffer.add_string(entry["Name"])
                buffer.add_string(entry["Unknown"])
                buffer.write(u32(buffer._string_refs[entry["Name"]]))
                buffer.write(u32(buffer._string_refs[entry["Unknown"]]))
                for slot in entry["Bones"]:
                    buffer.add_string(slot["Name"])
                    buffer.write(u32(buffer._string_refs[slot["Name"]]))
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
            for entry in self.calc_ctrl:
                flag = 0
                if "Command Data Type" in entry["Parameter"]:
                    flag |= 1 << 0x1F
                    flag |= entry["Parameter"]["Command Data Type"] & 0xFFFF
                else:
                    flag |= entry["Parameter"]["Blackboard Index"] & 0xFFFF
                buffer.write(u32(flag))
                buffer.write(f32(entry["Adjust Value"]))
                buffer.write(u32(Mode[entry["Calc Mode"]].value))
                buffer.write(f32(entry["Default Value"]))
                buffer.write(f32(entry["Adjust Rate"]))
                buffer.write(f32(entry["Base Result"]))
                buffer.write(f32(entry["Min"]))
                buffer.write(f32(entry["Max"]))
            buffer.write(u32(len(self.valid_tags)))
            for tag in self.valid_tags:
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
            buffer.write(u32(len(self.material_blend)))
            for entry in self.material_blend:
                buffer.add_string(entry["Name"])
                buffer.write(u32(buffer._string_refs[entry["Name"]]))
                buffer.write(f32(entry["Blend Start"]))
            buffer.write(u32(0)) # enum resolve
            buffer.write(buffer._strings)
            buffer.seek(0x50)
            buffer.write(u32(len(buffer._strings)))
            result_data_asb = buffer.stream.getvalue()
        result_baev_data = b""
        if self.has_asnode_baev:
            events = {}
            for node in self.nodes:
                if "BAEV Events" in node:
                    events["0x%08x" % calc_hash(node["GUID"])] = node["BAEV Events"]
            anim_events = BAEV.from_dict(events, self.filename)
            result_baev_data = anim_events.to_binary_stream()
        return result_data_asb, result_baev_data