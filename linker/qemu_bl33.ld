MEMORY
{
	image : ORIGIN = 0x0e100000, LENGTH = 4M
	payload (rwx) : ORIGIN = 0x0e500000, LENGTH = 64M
}

SECTIONS {
	.payload :
	{
       *(.payload);
       . = ALIGN(4);
     } > payload
} INSERT AFTER .text;
