_stext = 0x08006508;

SECTIONS
{
    .boot_version 0x08006500 :
    {
        LONG(0x00000999);
        LONG(0x00000000);
    } > FLASH
}
INSERT AFTER .vector_table;
