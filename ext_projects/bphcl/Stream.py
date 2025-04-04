import io
import struct

from util import _hex


class ReadStream(io.BytesIO):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.endian = "little"
        self.sign = "<" if self.endian == "little" else ">"
        self.max_str_size = 256
        self.encoding = "utf-8"
        
    def __repr__(self):
        return f"<ReadStream position={_hex(self.tell())}>"
    
    def __str__(self):
        return f"<ReadStream position=0x{self.tell():08X}>"

    def align_to(self, alignment: int = 8):
        """Align the stream's position to the next multiple of `alignment`."""
        current_pos = self.tell()
        if current_pos == 0:
            return
        padding = (alignment - (current_pos % alignment)) % alignment
        if padding:
            self.seek(padding, io.SEEK_CUR)
    
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

    def read_float(self) -> float:
        return self._read_and_unpack("f", 4)[0]

    def read_s128(self):
        return self._read_and_unpack("qq", 16)

    def read_u128(self):
        return self._read_and_unpack("QQ", 16)

    def read_u32(self):
        return self._read_and_unpack("I", 4)[0]
    
    def read_s32(self):
        return self._read_and_unpack("i", 4)[0]
    
    def read_u64(self):
        return self._read_and_unpack("Q", 8)[0]
    
    def read_u16(self):
        return self._read_and_unpack("H", 2)[0]
    
    def read_bool(self) -> bool:
        return self._read_and_unpack("?", 1)[0]
    
    def read_u8(self) -> int:
        return self._read_and_unpack("B", 1)[0]

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


class WriteStream(io.BytesIO):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.endian = "little"
        self.sign = "<" if self.endian == "little" else ">"
        self.encoding = "utf-8"
        
    def __repr__(self):
        return f"<WriteStream position=0x{self.tell():08X}>"
    
    def __str__(self):
        return f"<WriteStream position=0x{self.tell():08X}>"
    
    def align_to(self, alignment: int = 8, pad_byte: bytes = b'\x00'):
        """Pad the stream to the next multiple of `alignment`."""
        current_pos = self.tell()
        padding = (alignment - (current_pos % alignment)) % alignment
        if padding:
            self.write(pad_byte * padding)

    def _pack_and_write(self, fmt: str, *values):
        packed = struct.pack(self.sign + fmt, *values)
        self.write(packed)

    def write_float(self, val: float):
        self._pack_and_write("f", val)
    
    def write_u8(self, val: int):
        self._pack_and_write("B", val)
    
    def write_s8(self, val: int):
        self._pack_and_write("b", val)
    
    def write_u16(self, val: int):
        self._pack_and_write("H", val)

    def write_s16(self, val: int):
        self._pack_and_write("h", val)

    def write_u32(self, val: int):
        self._pack_and_write("I", val)
    
    def write_s32(self, val: int):
        self._pack_and_write("i", val)

    def write_u64(self, val: int):
        self._pack_and_write("Q", val)

    def write_s64(self, val: int):
        self._pack_and_write("q", val)

    def write_u128(self, val1: int, val2: int):
        """Write 128 bits as two 64-bit unsigned ints."""
        self.write_u64(val1)
        self.write_u64(val2)

    def write_s128(self, val1: int, val2: int):
        """Write 128 bits as two 64-bit signed ints."""
        self.write_s64(val1)
        self.write_s64(val2)
    
    def write_u8(self, val: int):
        self._pack_and_write("B", val)
    
    def write_bool(self, val: bool):
        self.write_u8(1 if val else 0)

    def write_bytes(self, data: bytes):
        self.write(data)

    def write_padding(self, count: int, pad_byte: bytes = b'\x00'):
        self.write(pad_byte * count)

    def write_string(self, text: str, null_terminated: bool = True):
        data = text.encode(self.encoding)
        self.write(data)
        if null_terminated:
            self.write(b'\x00')

    def write_string_w_size(self, text: str, total_size: int):
        data = text.encode(self.encoding)
        if len(data) > total_size:
            raise ValueError(f"String too long to fit in {total_size} bytes")
        self.write(data)
        self.write(b'\x00' * (total_size - len(data)))
