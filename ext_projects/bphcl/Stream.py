import io
import struct


from dataclasses import dataclass

class ReadStream(io.BytesIO):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.endian = "little"
        self.sign = "<" if self.endian == "little" else ">"
        self.max_str_size = 256
        self.encoding = "utf-8"
    
    def find_next_occ(self, data: bytes):
        """Find the next occurrence of the given data in the stream."""
        current_pos = self.tell()
        data_pos = self.read().find(data)
        result = -1 if data_pos == -1 else current_pos + data_pos
        self.seek(current_pos)
        return result
    
    def read_s128(self):
        data = self.read(16)
        if len(data) != 16:
            raise ValueError(f"End of file, expected 16 bytes, got {len(data)} bytes")
        return struct.unpack(self.sign + "q" * 2, data)
    
    def read_u128(self):
        data = self.read(16)
        if len(data) != 16:
            raise ValueError(f"End of file, expected 16 bytes, got {len(data)} bytes")
        return struct.unpack(self.sign + "Q" * 2, data)
    
    def read_u32(self):
        data = self.read(4)
        if len(data) != 4:
            raise ValueError(f"End of file, expected 4 bytes, got {len(data)} bytes")
        return struct.unpack(self.sign + "I", data)[0]
    
    def read_string_w_size(self, exp_size:int):
        rawdata = self.read(exp_size)
        if len(rawdata) != exp_size:
            raise ValueError(f"End of file, expected {exp_size} bytes, got {len(rawdata)} bytes")
        try:
            return rawdata.decode(self.encoding)
        except Exception as e:
            raise ValueError(f"Error decoding string from data {repr()}: {e}")
    
    def read_string(self):
        result = b""
        while True:
            size = len(result)
            byte = self.read(1)
            if len(byte) == 0:
                raise ValueError("End of file, expected 1 byte, got 0 bytes")
            if byte[0] > 128:
                return None
            if byte == b'\0':
                break
            if size >= self.max_str_size:
                raise ValueError(f"String too long, expected max {self.max_str_size} bytes, got {size} bytes")
            result += byte
        return result.decode(self.encoding)