_stext = 0x08003544;

SECTIONS
{
    .boot_version 0x08003530 :
    {
        LONG(0x00000999);
        BYTE(0x4D);
        BYTE(0x61);
        BYTE(0x69);
        BYTE(0x6E);
        LONG(ADDR(.text) + 1);
        LONG(0x20005000);
        LONG(ADDR(.text) + 1);
    } > FLASH
}
INSERT AFTER .vector_table;
