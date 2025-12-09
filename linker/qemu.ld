MEMORY
{
	image : ORIGIN = 0x40080000, LENGTH = 4M
	payload (rwx) : ORIGIN = 0x40400000, LENGTH = 64M
}

SECTIONS {
	.payload :
	{
       *(.payload);
       . = ALIGN(4);
     } > payload
} INSERT AFTER .text;
