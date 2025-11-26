MEMORY
{
	image : ORIGIN = 0x40080000, LENGTH = 4M
	heap (rw) : ORIGIN = 0x40400000, LENGTH = 4M
	payload (rwx) : ORIGIN = 0x40800000, LENGTH = 64M
}

SECTIONS {
     .heap :
	 {
       *(.heap);
       . = ALIGN(4);
     } > heap
	.payload :
	{
       *(.payload);
       . = ALIGN(4);
     } > payload
} INSERT AFTER .text;
