import io
import struct


class ReadStream(io.BytesIO):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.endian = "little"
        self.sign = "<" if self.endian == "little" else ">"
        self.max_str_size = 256
        self.encoding = "utf-8"
        
    def __repr__(self):
        return f"<ReadStream position=0x{self.tell():08X}>"
    
    def __str__(self):
        return f"<ReadStream position=0x{self.tell():08X}>"

    def _read_exact(self, size: int) -> bytes:
        data = self.read(size)
        if len(data) != size:
            raise EOFError(f"Expected {size} bytes, got {len(data)} bytes")
        return data

    def find_next_occ(self, data: bytes) -> int:
        current_pos = self.tell()
        found_pos = self.read().find(data)
        self.seek(current_pos)
        return -1 if found_pos == -1 else current_pos + found_pos

    def _read_and_unpack(self, fmt: str, size: int):
        data = self._read_exact(size)
        return struct.unpack(self.sign + fmt, data)

    def read_s128(self):
        return self._read_and_unpack("qq", 16)

    def read_u128(self):
        return self._read_and_unpack("QQ", 16)

    def read_u32(self):
        return self._read_and_unpack("I", 4)[0]

    def read_string_w_size(self, exp_size: int) -> str:
        data = self._read_exact(exp_size)
        try:
            return data.decode(self.encoding)
        except UnicodeDecodeError as e:
            raise ValueError(f"Failed to decode {exp_size} bytes using {self.encoding}: {e}")

    def read_string(self) -> str | None:
        MAX_ASCII = 128
        result = bytearray()
        while True:
            byte = self._read_exact(1)
            if byte[0] > MAX_ASCII:
                self.seek(-1, io.SEEK_CUR)  # Unread the byte
                return None
            if byte == b'\0':
                break
            if len(result) >= self.max_str_size:
                raise ValueError(f"String exceeds max length ({self.max_str_size} bytes)")
            result.append(byte[0])
        return result.decode(self.encoding)
