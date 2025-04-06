from dataclasses import asdict
from enum import Enum
import json
from pathlib import Path
import zlib
import re
import base64

class DataConverter:
    MAX_STR_LEN = 90000000  # Maximum allowed string length for hex/utf8 decoded data

    @staticmethod
    def _compress_if_data(key, value):
        if key == "data":
            _val = value if isinstance(value, bytes) else bytes.fromhex(value)
            compressed = b"ZLIB!" + zlib.compress(_val)
            return  base64.b64encode(compressed).decode('ascii') 
        return value
    
    @staticmethod
    def get_aamp_hashed_dict(path):
        text = Path(path).read_bytes()
        lines = [l for l in text.splitlines() if l.strip()]
        result = {}
        for line in lines:
            crc_value = str(crc32_decimal(line))
            entry = DataConverter.decode_bytes(line)
            result[crc_value] = entry
        return result

    @staticmethod
    def dataclass_to_dict(dataclass_instance):
        obj = asdict(dataclass_instance)
        return DataConverter.convert(obj)

    @staticmethod
    def convert(obj):
        if isinstance(obj, dict):
            return {
                DataConverter.convert_key(k): DataConverter._compress_if_data(k, DataConverter.convert(v))
                for k, v in obj.items()
                if v is not None and (not str(k).startswith("_") or k == "_str") and not DataConverter._is_large_string(v)
            }
        elif isinstance(obj, list):
            return [DataConverter.convert(v) for v in obj if v is not None and not DataConverter._is_large_string(v)]
        elif isinstance(obj, tuple):
            return tuple(DataConverter.convert(v) for v in obj if v is not None and not DataConverter._is_large_string(v))
        elif isinstance(obj, (bytes, bytearray)):
            return DataConverter.decode_bytes(obj)
        elif isinstance(obj, Enum):
            return obj.name
        else:
            return obj

    @staticmethod
    def convert_key(k):
        if isinstance(k, (bytes, bytearray)):
            return DataConverter.decode_bytes(k)
        elif isinstance(k, Enum):
            return k.name
        else:
            return k

    @staticmethod
    def decode_bytes(b):
        try:
            s = b.decode('utf-8')
        except UnicodeDecodeError:
            s = b.hex().upper()
        return None if len(s) > DataConverter.MAX_STR_LEN else s

    @staticmethod
    def reverse_convert(obj):
        if isinstance(obj, dict):
            return {
                DataConverter.reverse_convert_key(k): v
                for k, v in (
                    (k, DataConverter.reverse_convert(v)) for k, v in obj.items()
                )
                # if v is not None and k != "offset" and (not str(k).startswith("_") or k == "_str") and not DataConverter._is_large_string(v)
                if v is not None and (not str(k).startswith("_") or k == "_str") and not DataConverter._is_large_string(v)
            }
        elif isinstance(obj, list):
            return [DataConverter.reverse_convert(v) for v in obj if v is not None and not DataConverter._is_large_string(v)]
        elif isinstance(obj, tuple):
            return tuple(DataConverter.reverse_convert(v) for v in obj if v is not None and not DataConverter._is_large_string(v))
        elif isinstance(obj, str):
            return DataConverter.decode_string(obj)
        elif isinstance(obj, Enum):
            return obj.value
        else:
            return obj

    @staticmethod
    def reverse_convert_key(k):
        if isinstance(k, str):
            return DataConverter.decode_string(k)
        elif isinstance(k, Enum):
            return k.value
        else:
            return k

    @staticmethod
    def decode_string(s):
        try:
            return bytes.fromhex(s)
        except ValueError:
            try:
                return s.encode('utf-8')
            except UnicodeEncodeError:
                return s

    @staticmethod
    def _is_large_string(val):
        return isinstance(val, str) and len(val) > DataConverter.MAX_STR_LEN



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
  

# Example usage
if __name__ == "__main__":
  print("\n" * 20)
  _dict = DataConverter.get_aamp_hashed_dict("totk_botw_all_strings.txt")
  Path("tmp/tmp.json").write_text(json.dumps(_dict, indent=4))
  
    # Example string containing hex and decimal numbers
  input_strings = """""".splitlines()
  if input_strings:
    for i, input_string in enumerate(input_strings):
        res = extract_utf8_strings(input_string)
        if res: print(i+1, res)  # Output: ['H', 'e', 'l', 'l', 'o', '!']
