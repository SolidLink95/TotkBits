from dataclasses import asdict, dataclass, fields, is_dataclass, field
from enum import Enum
import json
from pathlib import Path
import zlib
import re
import base64
from typing import Any, List

class DataConverter:
    def __init__(self):
        self.MAX_STR_LEN = 90000000  # Maximum allowed string length for hex/utf8 decoded data


    def compare_dataclasses(self,a, b, path="root"):
        if type(a) != type(b):
            raise ValueError(f"Type mismatch at {path}: {type(a).__name__} != {type(b).__name__}")

        if not is_dataclass(a):
            if a != b:
                raise ValueError(f"Value mismatch at {path}: {a!r} != {b!r}")
            return

        for field in fields(a):
            val_a = getattr(a, field.name)
            val_b = getattr(b, field.name)
            new_path = f"{path}.{field.name}"

            # Recursively compare
            try:
                if is_dataclass(val_a):
                    self.compare_dataclasses(val_a, val_b, new_path)
                elif isinstance(val_a, list) and isinstance(val_b, list):
                    if len(val_a) != len(val_b):
                        raise ValueError(f"List length mismatch at {new_path}: {len(val_a)} != {len(val_b)}")
                    for i, (item_a, item_b) in enumerate(zip(val_a, val_b)):
                        self.compare_dataclasses(item_a, item_b, f"{new_path}[{i}]")
                else:
                    if val_a != val_b:
                        raise ValueError(f"Mismatch at {new_path}: {val_a!r} != {val_b!r}")
            except ValueError as e:
                print(f"Mismatch in dataclass '{type(a).__name__}' at {new_path}")
                print(a)
                raise

    
    def test_check_translation(self, obj):
        if isinstance(obj, dict):
            new_obj = {}
            for key, value in obj.items():
                if key == "m_translation" and isinstance(value, dict):
                    for _entry in ["m_x", "m_y", "m_z", "m_w"]:
                        if _entry not in value:
                            raise ValueError(f"Missing key '{_entry}' in m_translation dictionary")
                else:
                    new_obj[key] = self.test_check_translation(value)
            return new_obj

        elif isinstance(obj, list):
            return [self.test_check_translation(item) for item in obj]

        else:
            return obj  # return as-is if not dict or list
    def convert_offsets_to_hex(self, obj):
        """Recursively convert offsets in a dictionary or list to hex format."""
        if isinstance(obj, dict):
            new_obj = {}
            for key, value in obj.items():
                if key == "offset" and isinstance(value, int):
                    new_obj[key] = _hex(value)
                else:
                    new_obj[key] = self.convert_offsets_to_hex(value)
            return new_obj

        elif isinstance(obj, list):
            return [self.convert_offsets_to_hex(item) for item in obj]

        else:
            return obj  # return as-is if not dict or list
    
    def handle_value(self, key, value):
        if key == "data":
            _val = value if isinstance(value, bytes) else bytes.fromhex(value)
            compressed = b"ZLIB!" + zlib.compress(_val)
            return base64.b64encode(compressed).decode('ascii') 
        if key in ("signature", "magic") and isinstance(value, bytes):
            return value.decode()
        return value

    def get_aamp_hashed_dict(self, path):
        text = Path(path).read_bytes()
        lines = [l for l in text.splitlines() if l.strip()]
        result = {}
        for line in lines:
            crc_value = str(crc32_decimal(line))
            entry = self.decode_bytes(line)
            result[crc_value] = entry
        return result

    def dataclass_to_dict(self, dataclass_instance):
        obj = asdict(dataclass_instance)
        return self.convert(obj)
    
    def dict_to_dataclass(self, _dict):
        # obj = asdict(dataclass_instance)
        return self.reverse_convert(_dict)

    def convert(self, obj):
        if isinstance(obj, dict):
            return {
                self.convert_key(k): self.handle_value(k, self.convert(v))
                for k, v in obj.items()
                if v is not None and (not str(k).startswith("_") or k == "_str") and not self._is_large_string(v)
            }
        elif isinstance(obj, list):
            return [self.convert(v) for v in obj if v is not None and not self._is_large_string(v)]
        elif isinstance(obj, tuple):
            return tuple(self.convert(v) for v in obj if v is not None and not self._is_large_string(v))
        # elif isinstance(obj, (bytes, bytearray)):
        #     return self.decode_bytes(obj)
        elif isinstance(obj, Enum):
            return obj.name
        else:
            return obj

    def convert_key(self, k):
        # if isinstance(k, (bytes, bytearray)):
        #     return self.decode_bytes(k)
        if isinstance(k, Enum):
            return k.name
        else:
            return k

    def decode_bytes(self, b):
        try:
            s = b.decode('utf-8')
        except UnicodeDecodeError:
            s = b.hex().upper()
        return None if len(s) > self.MAX_STR_LEN else s

    def reverse_convert(self, obj):
        if isinstance(obj, dict):
            return {
                self.reverse_convert_key(k): v
                for k, v in (
                    (k, self.reverse_convert(v)) for k, v in obj.items()
                )
                if v is not None and (not str(k).startswith("_") or k == "_str") and not self._is_large_string(v)
            }
        elif isinstance(obj, list):
            return [self.reverse_convert(v) for v in obj if v is not None and not self._is_large_string(v)]
        elif isinstance(obj, tuple):
            return tuple(self.reverse_convert(v) for v in obj if v is not None and not self._is_large_string(v))
        elif isinstance(obj, str):
            return self.decode_string(obj)
        elif isinstance(obj, Enum):
            return obj.value
        else:
            return obj

    def reverse_convert_key(self, k):
        if isinstance(k, str):
            # return self.decode_string(k)
            return k
        elif isinstance(k, Enum):
            return k.value
        else:
            return k

    def decode_string(self, s):
        try:
            return bytes.fromhex(s)
        except ValueError:
            try:
                return s.encode('utf-8')
            except UnicodeEncodeError:
                return s

    def _is_large_string(self, val):
        return isinstance(val, str) and len(val) > self.MAX_STR_LEN


def _hex(val):
    return hex(val).upper().replace("0X", "0x")

def is_int_based_class(cls) -> bool:
    return getattr(cls, "_sign", None) is not None


def crc32_decimal(s: str) -> int:
    x = s.encode('utf-8') if isinstance(s, str) else s
    return zlib.crc32(x) & 0xFFFFFFFF  # Ensure unsigned


def extract_utf8_strings(s):
    # Find all hex (e.g., 0x41) and decimal numbers
    matches = re.findall(r'0[xX][0-9a-fA-F]+|\d+', s)
    decoded_strings = []

    for match in matches:
        try:
            # Parse as int from hex or decimal
            value = int(match, 16) if match.lower().startswith("0x") else int(match)

            # Try decoding as UTF-8 from bytes
            b = value.to_bytes((value.bit_length() + 7) // 8 or 1, 'big')
            decoded = b.decode('utf-8')
            decoded_strings.append(decoded)
        except (ValueError, UnicodeDecodeError, OverflowError):
            continue
    decoded_strings = [e for e in decoded_strings if e.isprintable() and len(e.strip()) > 1]
    return decoded_strings

def fix_hkarrays(s):
  lines = [l for l in s.splitlines() if l.strip()]
  _skips = ["super",".align_to"]
  hkarrays = {}
  _dict = {}
  res = ""
  returnline = next((l for l in lines if "return" in l), None)
  # assert returnline, "No return line found"
  for line in lines:
    if "return" in line:
      continue
    if any([skip in line for skip in _skips]):
      res += line + "\n"
      continue
    assert line.count('=') == 1, f"Invalid line: {line}"
    elems =  tuple([l.strip() for l in line.split('=') if l.strip()])
    assert(len(elems) == 2), f"Invalid line: {line}"
    var, fun = elems
    assert(var not in _dict), f"Duplicate hkArray: {var}"
    _dict[var] = fun
    indent = line.split(var)[0]
    if fun.startswith("hkArray.from_reader("):
      hkarrays[var] = fun
      new_fun = fun.replace("hkArray.from_reader(", "hkArray.get_default_array(")
      res += f"{indent}{var} = {new_fun}\n"
    else:
      res += f"{indent}{var} = {fun}\n"
  res += "\n"
  for var, fun in hkarrays.items():
    res += f"{indent}{var}.from_reader_self(stream)\n"
  if returnline:
    res += returnline + "\n"
  return res
  

def find_nested_instances(obj: Any, objtype: Any) -> List[Any]:
    results = []

    if isinstance(obj, objtype):
        results.append(obj)
    elif is_dataclass(obj):
        for f in fields(obj):
            value = getattr(obj, f.name, None)
            if value is not None:
                results.extend(find_nested_instances(value, objtype))
    elif isinstance(obj, list):
        for item in obj:
            results.extend(find_nested_instances(item, objtype))
    elif isinstance(obj, dict):
        for key, value in obj.items():
            results.extend(find_nested_instances(key, objtype))
            results.extend(find_nested_instances(value, objtype))
    
    return results

def is_offset_in_ranges_list(offset: int, ranges: list) -> bool:
    # _ranges = [(int(start, 16), int(end, 16)) for start, end in ranges]
    for start, end in ranges:
        if start <= offset <= end:
            return True
    return False

@dataclass
class FilteredInternalPath:
    index: int = -1
    internalPatch: Any = None
    offsets: list = field(default_factory=list)
    
    def __repr__(self):
        return f"InternalPath(index={self.index}, type_index={self.internalPatch.type_index}, off_count={len(self.offsets)})"

@dataclass
class AssetsFromRange:
    items: list = field(default_factory=list)
    named_types: list = field(default_factory=list)
    internal_patches: list = field(default_factory=list) # list[FilteredInternalPath]
    
    def __len__(self):
        return len(self.items) + len(self.named_types) + len(self.internal_patches)
    
    def sort(self):
        self.items.sort(key=lambda x: x[0])
        self.named_types.sort(key=lambda x: x[0])
        self.internal_patches.sort(key=lambda x: x.index)


class hexInt(int):
    def __repr__(self):
        return _hex(self)

    def __str__(self):
        return _hex(self)
# # Example usage
# if __name__ == "__main__":
#   print("\n" * 20)
#   _dict = DataConverter().get_aamp_hashed_dict("totk_botw_all_strings.txt")
#   Path("tmp/tmp.json").write_text(json.dumps(_dict, indent=4))
  
#     # Example string containing hex and decimal numbers
#   input_strings = """""".splitlines()
#   if input_strings:
#     for i, input_string in enumerate(input_strings):
#         res = extract_utf8_strings(input_string)
#         if res: print(i+1, res)  # Output: ['H', 'e', 'l', 'l', 'o', '!']
