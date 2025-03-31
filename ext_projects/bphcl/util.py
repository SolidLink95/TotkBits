

class DataConverter:
    
    @staticmethod
    def bytes_to_hex_string(data: bytes|bytearray) -> str:
        """Convert bytes to a hex string."""
        return data.hex().upper()
    
    @staticmethod
    def hex_string_to_bytes(hex_string: str) -> bytes:
        """Convert a hex string to bytes."""
        return bytes.fromhex(hex_string)