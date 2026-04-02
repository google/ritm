MEMORY
{
	image : ORIGIN = 0x60000000, LENGTH = 4M
	payload (rwx) : ORIGIN = 0x60400000, LENGTH = 64M
}

SECTIONS {
	.payload :
	{
       *(.payload);
       . = ALIGN(4);
     } > payload
} INSERT AFTER .text;
