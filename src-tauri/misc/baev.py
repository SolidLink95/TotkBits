try:
    from utils import *
except ImportError:
    raise ImportError("utils.py not found")

import json
import os

# Hash function for baev hashes
# The seed is the first string in the string pool (usually null)
# The string is either the node's GUID in the asb file or the animation's name
def calc_hash(string, seed=""):
    
    length = len(string)

    hash = 0x811c9dc5
    const = 0x1000193

    if seed != "":
        i = len(seed) & 3
        pos = 0
        if len(seed) - 1 > 2:
            while pos < len(seed) & 0xFFFFFFFFFFFFFFFC:
                a = ord(seed[pos])
                b = ord(seed[pos+1])
                c = ord(seed[pos+2])
                d = ord(seed[pos+3])
                hash = ((((hash ^ a) * const ^ b) * const ^ c) * const ^ d) * const
                pos += 4
        while i != 0:
            a = ord(string[pos])
            hash = (hash ^ a) * const
            pos += 1
            i -= 1

    pos = 0
    i = length & 3
    while i != 0:
        a = ord(string[pos])
        hash = (hash ^ a) * const
        pos += 1
        i -= 1
    if length - 1 > 2:
        while pos < length:
            a = ord(string[pos])
            b = ord(string[pos+1])
            c = ord(string[pos+2])
            d = ord(string[pos+3])
            hash = ((((hash ^ a) * const ^ b) * const ^ c) * const ^ d) * const
            pos += 4

    return hash & 0xFFFFFFFF

class BAEV:
    def __init__(self, data, filename, stream=ReadStream(b''), string_pool=ReadStream(b'')):
        if data:
            self.events = data
        else:
            self.events = {}
        self.filename = filename
        self.stream = stream
        self.string_pool = string_pool
        cwd = os.path.dirname(os.path.abspath(__file__))
        events_json = os.path.join(cwd, "events.json")
        with open(events_json, "r") as f:
            self.event_list = json.load(f)

    @classmethod
    def from_binary(cls, data, filename):
        assert type(data) in [bytes, bytearray], "Data should be bytes or bytearray"
        this = cls([], filename, ReadStream(data), ReadStream(data))
        
        header = this.read_file_header()
        this.stream.seek(header["Section Info"][0]["Base Offset"])
        container = this.read_container()
        this.events = {i["Hash"]: i["Nodes"] for i in container["Event Info"]}

        return this

    @classmethod
    def from_dict(cls, data, filename):
        assert type(data) == dict, "Data should be a dictionary"
        return cls(data, filename)
    
    def read_header(self):
        header = {}
        # should be BFFH (binary cafe file header) or BFSI (binary cafe section info)
        header["Magic"] = self.stream.read(4).decode('utf-8') 
        header["Section Offset"] = self.stream.read_u32()
        header["Section Size"] = self.stream.read_u32()
        header["Section Alignment"] = self.stream.read_u32()
        return header
    
    def read_file_header(self):
        header = self.read_header()
        header["Section Info"] = self.read_array(self.read_section_header)
        header["Container Offset"] = self.stream.read_u64()
        header["Meme"] = self.stream.read(0x80).replace(b'\x00', b'').decode('utf-8')
        return header
    
    def read_section_header(self):
        header = self.read_header()
        header["Base Offset"] = self.stream.read_u64()
        header["Section Name"] = self.stream.read(0x10).replace(b'\x00', b'').decode('utf-8')
        return header

    # Common BAEV array structure
    def read_array(self, element):
        array = []
        offset = self.stream.read_u64()
        count = self.stream.read_u32()
        size = self.stream.read_u32()
        pos = self.stream.tell()
        self.stream.seek(offset)
        for i in range(count):
            array.append(element())
        self.stream.seek(pos)
        return array
    
    def read_container(self):
        container = {}
        container["Head Offset"] = self.stream.read_u64()
        ver_sub = self.stream.read_u8()
        ver_min = self.stream.read_u8()
        ver_maj = self.stream.read_u16()
        container["Version"] = str(ver_maj) + "." + str(ver_min) + "." + str(ver_sub)
        container["Unknown Value"] = self.stream.read_u32()
        container["String Pool Offset"] = self.stream.read_u64()
        container["Event Info"] = self.read_array(self.read_node)
        nodes = self.read_array(self.read_event_node)
        # with open('test.json', 'w') as f:
        #     json.dump(nodes, f, indent=4)
        for entry in container["Event Info"]:
            for i in range(len(entry["Nodes"])):
                entry["Nodes"][i] = nodes[entry["Nodes"][i]]
        return container
    
    def read_node(self):
        entry = {}
        entry["Hash"] = "0x%08x" % self.stream.read_u32()
        padding = self.stream.read_u32()
        entry["Nodes"] = self.read_array(self.stream.read_u32) # indices
        return entry
    
    def read_event_node(self):
        entry = {}
        offset = self.stream.read_u64()
        count = self.stream.read_u32()
        entry_size = self.stream.read_u32()
        entry["Hash"] = "0x%08x" % self.stream.read_u32()
        entry["Unknown"] = self.stream.read_u32()
        pos = self.stream.tell()
        self.stream.seek(offset)
        entry["Event"] = {}
        for i in range(count):
            name, event = self.read_event()
            entry["Event"][name] = event
        self.stream.seek(pos)
        return entry

    def read_event(self):
        entry = {}
        name = self.string_pool.read_string(self.stream.read_u64())
        entry["Trigger Array"] = self.read_array(self.read_trigger_event_array)
        entry["Hold Array"] = self.read_array(self.read_hold_event_array)
        is_hold_event = bool(self.stream.read_u32())
        id = self.stream.read_u32()
        if entry["Trigger Array"] == []:
            del entry["Trigger Array"]
        if entry["Hold Array"] == []:
            del entry["Hold Array"]
        return name, entry

    def read_trigger_event_array(self):
        entry = {}
        entry["Parameters"] = self.read_array(self.read_param_offset)
        entry["Start Frame"] = self.stream.read_f32()
        padding = self.stream.read_f32()
        return entry
    
    def read_hold_event_array(self):
        entry = {}
        entry["Parameters"] = self.read_array(self.read_param_offset)
        entry["Start Frame"] = self.stream.read_f32()
        entry["End Frame"] = self.stream.read_f32()
        return entry

    def read_parameter(self):
        param_type = self.stream.read_u32()
        padding = self.stream.read_u32()
        match param_type:
            case 0:
                parameter = self.stream.read_u32()
            case 1:
                parameter = self.stream.read_f32()
            case 3:
                parameter = [self.stream.read_f32(), self.stream.read_f32(), self.stream.read_f32()]
            case 5:
                parameter = self.string_pool.read_string(self.stream.read_u64())
            case _:
                raise ValueError(param_type, hex(self.stream.tell()))
        return parameter

    def read_param_offset(self):
        offset = self.stream.read_u64()

        pos = self.stream.tell()
        self.stream.seek(offset)
        param = self.read_parameter()
        self.stream.seek(pos)
        return param
    
    def to_json(self, output_dir=""):
        if output_dir:
            os.makedirs(output_dir, exist_ok=True)
        with open(os.path.join(output_dir, self.filename + ".json"), "w", encoding="utf-8") as f:
            json.dump(self.events, f, indent=4, ensure_ascii=False)

    def calc_offsets(self, buffer):
        offsets = {}
        offset = 0
        offsets["BFFH"] = offset
        offset += 0xA8
        offsets["BFSI0"] = offset
        offset += 0x28
        offsets["BFSI1"] = offset
        offset += 0x28
        offsets["Container"] = offset
        offset += 0x38
        offsets["HashHeader"] = offset
        offset += 0x18 * len(self.events)
        offsets["Indices"] = offset
        for event in self.events:
            offset += 0x4 * len(self.events[event])
        offset = offset - offset % 8 + (8 if offset % 8 != 0 else 0)
        offsets["Nodes"] = offset
        for entry in self.events:
            offset += 0x18 * len(self.events[entry])
        offsets["Events"] = offset
        count = 0
        for entry in self.events:
            for node in self.events[entry]:
                count += 1
                for event in node["Event"]:
                    offset += 0x30
                    buffer.add_string(event)
                    if "Trigger Array" in node["Event"][event]:
                        for trigger in node["Event"][event]["Trigger Array"]:
                            offset += 0x18
                            for param in trigger["Parameters"]:
                                offset += 0x8 + (0x18 if type(param) == list else 0x10)
                                if type(param) == str:
                                    buffer.add_string(param)
                    if "Hold Array" in node["Event"][event]:
                        for hold in node["Event"][event]["Hold Array"]:
                            offset += 0x18
                            for param in hold["Parameters"]:
                                offset += 0x8 + (0x18 if type(param) == list else 0x10)
                                if type(param) == str:
                                    buffer.add_string(param)
        offsets["String"] = offset
        offsets["Size"] = offset + len(buffer._strings)
        return offsets, count

    def to_binary(self, output_dir=""):
        if output_dir:
            os.makedirs(output_dir, exist_ok=True)
        with open(os.path.join(output_dir, self.filename + ".baev"), "wb") as f:
            buffer = WriteStream(f)
            buffer.add_string("")
            offsets, count = self.calc_offsets(buffer)
            buffer.write("BFFH".encode('utf-8'))
            buffer.write(u32(offsets["BFFH"]))
            buffer.write(u32(offsets["Size"])) # file size
            buffer.write(u32(8)) # alignment
            buffer.write(u64(offsets["BFSI0"])) # offset for section headers array
            buffer.write(u32(2)) # section count
            buffer.write(u32(0x28)) # section header size
            buffer.write(u64(offsets["Container"])) # data container offset
            buffer.write("Nintendo.AnimationEvent.ResourceConverter.Resource.AnimationEventArchiveResData".encode('utf-8'))
            buffer.skip(0x31) # padding bc the meme string is 0x80 bytes
            buffer.write("BFSI".encode('utf-8'))
            buffer.write(u32(offsets["Container"])) # section offset
            buffer.write(u32(offsets["String"] - offsets["Container"])) # section size
            buffer.write(u32(8)) # section alignment
            buffer.write(u64(offsets["Container"])) # section pointer
            buffer.write("Default\x00\x00\x00\x00\x00\x00\x00\x00\x00".encode('utf-8')) # section name string
            buffer.write("BFSI".encode('utf-8')) # header is the same as the previous one
            buffer.write(u32(offsets["String"]))
            buffer.write(u32(len(buffer._strings)))
            buffer.write(u32(1))
            buffer.write(u64(offsets["String"]))
            buffer.write("StringPool\x00\x00\x00\x00\x00\x00".encode('utf-8'))
            buffer.write(u64(0))
            buffer.write(b'\x00\x00\x01\x00') # version
            buffer.write(u32(0)) # padding
            buffer.write(u64(offsets["String"])) # string pool offset
            if (len(self.events) == 0):
                buffer.write(u64(0))
                buffer.write(u32(0))
                buffer.write(u32(0))
            else:
                buffer.write(u64(offsets["HashHeader"]))
                buffer.write(u32(len(self.events)))
                buffer.write(u32(0x18)) # element size
                
            if (count == 0):
                buffer.write(u64(0))
                buffer.write(u32(0))
                buffer.write(u32(0))
            else:
                buffer.write(u64(offsets["Nodes"]))
                buffer.write(u32(count))
                buffer.write(u32(0x18))
            offset = offsets["Indices"]
            sorted_events = dict(sorted(self.events.items()))
            for entry in sorted_events:
                buffer.write(u32(int(entry, 16)))
                buffer.write(u32(0)) # padding
                buffer.write(u64(offset))
                offset += 4 * len(sorted_events[entry])
                buffer.write(u32(len(sorted_events[entry])))
                buffer.write(u32(4)) # entry size
            # ideally we'd want to figure out how these are sorted because this isn't right
            nodes = []
            for entry in self.events:
                for node in self.events[entry]:
                    nodes.append(node)
            for entry in sorted_events:
                for node in sorted_events[entry]:
                    buffer.write(u32(nodes.index(node)))
            while buffer.tell() % 8 != 0:
                buffer.write(u8(0))
            offset = offsets["Events"]
            for node in nodes:
                buffer.write(u64(offset))
                buffer.write(u32(len(node["Event"])))
                buffer.write(u32(0x30)) # element size
                buffer.write(u32(int(node["Hash"], 16)))
                buffer.write(u32(node["Unknown"]))
                offset += 0x30 * len(node["Event"])
                for event in node["Event"]:
                    if "Trigger Array" in node["Event"][event]:
                        for trigger in node["Event"][event]["Trigger Array"]:
                            offset += 0x18
                            for param in trigger["Parameters"]:
                                offset += 0x8 + (0x18 if type(param) == list else 0x10)
                    if "Hold Array" in node["Event"][event]:
                        for hold in node["Event"][event]["Hold Array"]:
                            offset += 0x18
                            for param in hold["Parameters"]:
                                offset += 0x8 + (0x18 if type(param) == list else 0x10)
            for node in nodes:
                offset = buffer.tell() + 0x30 * len(node["Event"])
                for event in node["Event"]:
                    buffer.write(u64(buffer._string_refs[event] + offsets["String"]))
                    if "Trigger Array" in node["Event"][event]:
                        buffer.write(u64(offset))
                        buffer.write(u32(len(node["Event"][event]["Trigger Array"])))
                        buffer.write(u32(0x18))
                        offset += 0x18 * len(node["Event"][event]["Trigger Array"])
                    else:
                        buffer.write(u64(0))
                        buffer.write(u32(0))
                        buffer.write(u32(0))
                    if "Hold Array" in node["Event"][event]:
                        buffer.write(u64(offset))
                        buffer.write(u32(len(node["Event"][event]["Hold Array"])))
                        buffer.write(u32(0x18))
                        offset += 0x18 * len(node["Event"][event]["Hold Array"])
                    else:
                        buffer.write(u64(0))
                        buffer.write(u32(0))
                        buffer.write(u32(0))
                    buffer.write(u32(1 if "Hold Array" in node["Event"][event] else 0))
                    if "Trigger Array" in node["Event"][event]:
                        buffer.write(u32(self.event_list["Trigger"].index(event)))
                    elif "Hold Array" in node["Event"][event]:
                        buffer.write(u32(self.event_list["Hold"].index(event)))
                    elif event in self.event_list["Trigger"]:
                        buffer.write(u32(self.event_list["Trigger"].index(event)))
                    elif event in self.event_list["Hold"]:
                        buffer.write(u32(self.event_list["Hold"].index(event)))
                    else:
                        buffer.write(u32(0))
                for event in node["Event"]:
                    if "Trigger Array" in node["Event"][event]:
                        for trigger in node["Event"][event]["Trigger Array"]:
                            if trigger["Parameters"] != []:
                                buffer.write(u64(offset))
                                offset += 8 * len(trigger["Parameters"])
                            else:
                                buffer.write(u64(0))
                            buffer.write(u32(len(trigger["Parameters"])))
                            has_vec = False
                            for param in trigger["Parameters"]:
                                if type(param) == list:
                                    has_vec = True
                                    break
                            if has_vec:
                                buffer.write(u32(0x10))
                            else:
                                buffer.write(u32(0x8))
                            buffer.write(f32(trigger["Start Frame"]))
                            buffer.write(f32(0))
                    if "Hold Array" in node["Event"][event]:
                        for hold in node["Event"][event]["Hold Array"]:
                            if hold["Parameters"] != []:
                                buffer.write(u64(offset))
                                offset += 8 * len(hold["Parameters"])
                            else:
                                buffer.write(u64(0))
                            buffer.write(u32(len(hold["Parameters"])))
                            has_vec = False
                            for param in hold["Parameters"]:
                                if type(param) == list:
                                    has_vec = True
                                    break
                            if has_vec:
                                buffer.write(u32(0x10))
                            else:
                                buffer.write(u32(0x8))
                            buffer.write(f32(hold["Start Frame"]))
                            buffer.write(f32(hold["End Frame"]))
                offset = buffer.tell()
                for event in node["Event"]:
                    if "Trigger Array" in node["Event"][event]:
                        for trigger in node["Event"][event]["Trigger Array"]:
                            offset += 8 * len(trigger["Parameters"])
                    if "Hold Array" in node["Event"][event]:
                        for hold in node["Event"][event]["Hold Array"]:
                            offset += 8 * len(hold["Parameters"])
                for event in node["Event"]:
                    if "Trigger Array" in node["Event"][event]:
                        for trigger in node["Event"][event]["Trigger Array"]:
                            if trigger["Parameters"]:
                                for param in trigger["Parameters"]:
                                    buffer.write(u64(offset))
                                    offset += 0x18 if type(param) == list else 0x10
                    if "Hold Array" in node["Event"][event]:
                        for hold in node["Event"][event]["Hold Array"]:
                            if hold["Parameters"]:
                                for param in hold["Parameters"]:
                                    buffer.write(u64(offset))
                                    offset += 0x18 if type(param) == list else 0x10
                for event in node["Event"]:
                    if "Trigger Array" in node["Event"][event]:
                        for trigger in node["Event"][event]["Trigger Array"]:
                            for param in trigger["Parameters"]:
                                if type(param) == int:
                                    buffer.write(u32(0))
                                    buffer.write(u32(0))
                                    buffer.write(u32(param))
                                    buffer.write(u32(0))
                                elif type(param) == float:
                                    buffer.write(u32(1))
                                    buffer.write(u32(0))
                                    buffer.write(f32(param))
                                    buffer.write(u32(0))
                                elif type(param) == list:
                                    buffer.write(u32(3))
                                    buffer.write(u32(0))
                                    buffer.write(f32(param[0]))
                                    buffer.write(f32(param[1]))
                                    buffer.write(f32(param[2]))
                                    buffer.write(u32(0))
                                elif type(param) == str:
                                    buffer.write(u32(5))
                                    buffer.write(u32(0))
                                    buffer.write(u64(buffer._string_refs[param] + offsets["String"]))
                                else:
                                    raise ValueError("Invalid Parameter Type")
                    if "Hold Array" in node["Event"][event]:
                        for hold in node["Event"][event]["Hold Array"]:
                            for param in hold["Parameters"]:
                                if type(param) == int:
                                    buffer.write(u32(0))
                                    buffer.write(u32(0))
                                    buffer.write(u32(param))
                                    buffer.write(u32(0))
                                elif type(param) == float:
                                    buffer.write(u32(1))
                                    buffer.write(u32(0))
                                    buffer.write(f32(param))
                                    buffer.write(u32(0))
                                elif type(param) == list:
                                    buffer.write(u32(3))
                                    buffer.write(u32(0))
                                    buffer.write(f32(param[0]))
                                    buffer.write(f32(param[1]))
                                    buffer.write(f32(param[2]))
                                    buffer.write(u32(0))
                                elif type(param) == str:
                                    buffer.write(u32(5))
                                    buffer.write(u32(0))
                                    buffer.write(u64(buffer._string_refs[param] + offsets["String"]))
                                else:
                                    raise ValueError("Invalid Parameter Type")
            assert buffer.tell() == offsets["String"], f"{hex(buffer.tell())}, expected {hex(offsets['String'])}"
            buffer.write(buffer._strings)
            
    
    def to_binary_stream(self, ):
        with io.BytesIO() as f:
            buffer = WriteStream(f)
            buffer.add_string("")
            offsets, count = self.calc_offsets(buffer)
            buffer.write("BFFH".encode('utf-8'))
            buffer.write(u32(offsets["BFFH"]))
            buffer.write(u32(offsets["Size"])) # file size
            buffer.write(u32(8)) # alignment
            buffer.write(u64(offsets["BFSI0"])) # offset for section headers array
            buffer.write(u32(2)) # section count
            buffer.write(u32(0x28)) # section header size
            buffer.write(u64(offsets["Container"])) # data container offset
            buffer.write("Nintendo.AnimationEvent.ResourceConverter.Resource.AnimationEventArchiveResData".encode('utf-8'))
            buffer.skip(0x31) # padding bc the meme string is 0x80 bytes
            buffer.write("BFSI".encode('utf-8'))
            buffer.write(u32(offsets["Container"])) # section offset
            buffer.write(u32(offsets["String"] - offsets["Container"])) # section size
            buffer.write(u32(8)) # section alignment
            buffer.write(u64(offsets["Container"])) # section pointer
            buffer.write("Default\x00\x00\x00\x00\x00\x00\x00\x00\x00".encode('utf-8')) # section name string
            buffer.write("BFSI".encode('utf-8')) # header is the same as the previous one
            buffer.write(u32(offsets["String"]))
            buffer.write(u32(len(buffer._strings)))
            buffer.write(u32(1))
            buffer.write(u64(offsets["String"]))
            buffer.write("StringPool\x00\x00\x00\x00\x00\x00".encode('utf-8'))
            buffer.write(u64(0))
            buffer.write(b'\x00\x00\x01\x00') # version
            buffer.write(u32(0)) # padding
            buffer.write(u64(offsets["String"])) # string pool offset
            if (len(self.events) == 0):
                buffer.write(u64(0))
                buffer.write(u32(0))
                buffer.write(u32(0))
            else:
                buffer.write(u64(offsets["HashHeader"]))
                buffer.write(u32(len(self.events)))
                buffer.write(u32(0x18)) # element size
                
            if (count == 0):
                buffer.write(u64(0))
                buffer.write(u32(0))
                buffer.write(u32(0))
            else:
                buffer.write(u64(offsets["Nodes"]))
                buffer.write(u32(count))
                buffer.write(u32(0x18))
            offset = offsets["Indices"]
            sorted_events = dict(sorted(self.events.items()))
            for entry in sorted_events:
                buffer.write(u32(int(entry, 16)))
                buffer.write(u32(0)) # padding
                buffer.write(u64(offset))
                offset += 4 * len(sorted_events[entry])
                buffer.write(u32(len(sorted_events[entry])))
                buffer.write(u32(4)) # entry size
            # ideally we'd want to figure out how these are sorted because this isn't right
            nodes = []
            for entry in self.events:
                for node in self.events[entry]:
                    nodes.append(node)
            for entry in sorted_events:
                for node in sorted_events[entry]:
                    buffer.write(u32(nodes.index(node)))
            while buffer.tell() % 8 != 0:
                buffer.write(u8(0))
            offset = offsets["Events"]
            for node in nodes:
                buffer.write(u64(offset))
                buffer.write(u32(len(node["Event"])))
                buffer.write(u32(0x30)) # element size
                buffer.write(u32(int(node["Hash"], 16)))
                buffer.write(u32(node["Unknown"]))
                offset += 0x30 * len(node["Event"])
                for event in node["Event"]:
                    if "Trigger Array" in node["Event"][event]:
                        for trigger in node["Event"][event]["Trigger Array"]:
                            offset += 0x18
                            for param in trigger["Parameters"]:
                                offset += 0x8 + (0x18 if type(param) == list else 0x10)
                    if "Hold Array" in node["Event"][event]:
                        for hold in node["Event"][event]["Hold Array"]:
                            offset += 0x18
                            for param in hold["Parameters"]:
                                offset += 0x8 + (0x18 if type(param) == list else 0x10)
            for node in nodes:
                offset = buffer.tell() + 0x30 * len(node["Event"])
                for event in node["Event"]:
                    buffer.write(u64(buffer._string_refs[event] + offsets["String"]))
                    if "Trigger Array" in node["Event"][event]:
                        buffer.write(u64(offset))
                        buffer.write(u32(len(node["Event"][event]["Trigger Array"])))
                        buffer.write(u32(0x18))
                        offset += 0x18 * len(node["Event"][event]["Trigger Array"])
                    else:
                        buffer.write(u64(0))
                        buffer.write(u32(0))
                        buffer.write(u32(0))
                    if "Hold Array" in node["Event"][event]:
                        buffer.write(u64(offset))
                        buffer.write(u32(len(node["Event"][event]["Hold Array"])))
                        buffer.write(u32(0x18))
                        offset += 0x18 * len(node["Event"][event]["Hold Array"])
                    else:
                        buffer.write(u64(0))
                        buffer.write(u32(0))
                        buffer.write(u32(0))
                    buffer.write(u32(1 if "Hold Array" in node["Event"][event] else 0))
                    if "Trigger Array" in node["Event"][event]:
                        buffer.write(u32(self.event_list["Trigger"].index(event)))
                    elif "Hold Array" in node["Event"][event]:
                        buffer.write(u32(self.event_list["Hold"].index(event)))
                    elif event in self.event_list["Trigger"]:
                        buffer.write(u32(self.event_list["Trigger"].index(event)))
                    elif event in self.event_list["Hold"]:
                        buffer.write(u32(self.event_list["Hold"].index(event)))
                    else:
                        buffer.write(u32(0))
                for event in node["Event"]:
                    if "Trigger Array" in node["Event"][event]:
                        for trigger in node["Event"][event]["Trigger Array"]:
                            if trigger["Parameters"] != []:
                                buffer.write(u64(offset))
                                offset += 8 * len(trigger["Parameters"])
                            else:
                                buffer.write(u64(0))
                            buffer.write(u32(len(trigger["Parameters"])))
                            has_vec = False
                            for param in trigger["Parameters"]:
                                if type(param) == list:
                                    has_vec = True
                                    break
                            if has_vec:
                                buffer.write(u32(0x10))
                            else:
                                buffer.write(u32(0x8))
                            buffer.write(f32(trigger["Start Frame"]))
                            buffer.write(f32(0))
                    if "Hold Array" in node["Event"][event]:
                        for hold in node["Event"][event]["Hold Array"]:
                            if hold["Parameters"] != []:
                                buffer.write(u64(offset))
                                offset += 8 * len(hold["Parameters"])
                            else:
                                buffer.write(u64(0))
                            buffer.write(u32(len(hold["Parameters"])))
                            has_vec = False
                            for param in hold["Parameters"]:
                                if type(param) == list:
                                    has_vec = True
                                    break
                            if has_vec:
                                buffer.write(u32(0x10))
                            else:
                                buffer.write(u32(0x8))
                            buffer.write(f32(hold["Start Frame"]))
                            buffer.write(f32(hold["End Frame"]))
                offset = buffer.tell()
                for event in node["Event"]:
                    if "Trigger Array" in node["Event"][event]:
                        for trigger in node["Event"][event]["Trigger Array"]:
                            offset += 8 * len(trigger["Parameters"])
                    if "Hold Array" in node["Event"][event]:
                        for hold in node["Event"][event]["Hold Array"]:
                            offset += 8 * len(hold["Parameters"])
                for event in node["Event"]:
                    if "Trigger Array" in node["Event"][event]:
                        for trigger in node["Event"][event]["Trigger Array"]:
                            if trigger["Parameters"]:
                                for param in trigger["Parameters"]:
                                    buffer.write(u64(offset))
                                    offset += 0x18 if type(param) == list else 0x10
                    if "Hold Array" in node["Event"][event]:
                        for hold in node["Event"][event]["Hold Array"]:
                            if hold["Parameters"]:
                                for param in hold["Parameters"]:
                                    buffer.write(u64(offset))
                                    offset += 0x18 if type(param) == list else 0x10
                for event in node["Event"]:
                    if "Trigger Array" in node["Event"][event]:
                        for trigger in node["Event"][event]["Trigger Array"]:
                            for param in trigger["Parameters"]:
                                if type(param) == int:
                                    buffer.write(u32(0))
                                    buffer.write(u32(0))
                                    buffer.write(u32(param))
                                    buffer.write(u32(0))
                                elif type(param) == float:
                                    buffer.write(u32(1))
                                    buffer.write(u32(0))
                                    buffer.write(f32(param))
                                    buffer.write(u32(0))
                                elif type(param) == list:
                                    buffer.write(u32(3))
                                    buffer.write(u32(0))
                                    buffer.write(f32(param[0]))
                                    buffer.write(f32(param[1]))
                                    buffer.write(f32(param[2]))
                                    buffer.write(u32(0))
                                elif type(param) == str:
                                    buffer.write(u32(5))
                                    buffer.write(u32(0))
                                    buffer.write(u64(buffer._string_refs[param] + offsets["String"]))
                                else:
                                    raise ValueError("Invalid Parameter Type")
                    if "Hold Array" in node["Event"][event]:
                        for hold in node["Event"][event]["Hold Array"]:
                            for param in hold["Parameters"]:
                                if type(param) == int:
                                    buffer.write(u32(0))
                                    buffer.write(u32(0))
                                    buffer.write(u32(param))
                                    buffer.write(u32(0))
                                elif type(param) == float:
                                    buffer.write(u32(1))
                                    buffer.write(u32(0))
                                    buffer.write(f32(param))
                                    buffer.write(u32(0))
                                elif type(param) == list:
                                    buffer.write(u32(3))
                                    buffer.write(u32(0))
                                    buffer.write(f32(param[0]))
                                    buffer.write(f32(param[1]))
                                    buffer.write(f32(param[2]))
                                    buffer.write(u32(0))
                                elif type(param) == str:
                                    buffer.write(u32(5))
                                    buffer.write(u32(0))
                                    buffer.write(u64(buffer._string_refs[param] + offsets["String"]))
                                else:
                                    raise ValueError("Invalid Parameter Type")
            assert buffer.tell() == offsets["String"], f"{hex(buffer.tell())}, expected {hex(offsets['String'])}"
            buffer.write(buffer._strings)

            return buffer.stream.getvalue()