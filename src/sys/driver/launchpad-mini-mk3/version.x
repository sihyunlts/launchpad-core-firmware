_stext = 0x0800C200;

SECTIONS
{
    .boot_version 0x0800C1E8 :
    {
        BYTE(0xE7);
        BYTE(0xD1);
        BYTE(0x30);
        BYTE(0xBC);
        BYTE(0x70);
        BYTE(0x47);
        BYTE(0x00);
        BYTE(0x20);
        BYTE(0x70);
        BYTE(0x47);
        BYTE(0xFF);
        BYTE(0xFF);
        BYTE(0xFF);
        BYTE(0xFF);
        BYTE(0xFF);
        BYTE(0xFF);
        BYTE(0x30);
        BYTE(0x30);
        BYTE(0x30);
        BYTE(0x39);
        BYTE(0x39);
        BYTE(0x39);
        BYTE(0x00);
        BYTE(0x00);
    } > FLASH
}
INSERT AFTER .vector_table;
