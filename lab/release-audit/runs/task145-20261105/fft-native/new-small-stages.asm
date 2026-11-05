
target/task145/fft-target/release/fft-native-candidate:     file format elf64-x86-64


Disassembly of section .text:

00000000000d9990 <_RNvNtNtNtCs3js4z49Ikqb_4syom6engine5imdct7avx512k18ifft_stages_avx512>:
   d9990:	55                   	push   rbp
   d9991:	41 57                	push   r15
   d9993:	41 56                	push   r14
   d9995:	41 55                	push   r13
   d9997:	41 54                	push   r12
   d9999:	53                   	push   rbx
   d999a:	48 81 ec 98 00 00 00 	sub    rsp,0x98
   d99a1:	48 83 fe 02          	cmp    rsi,0x2
   d99a5:	0f 82 02 09 00 00    	jb     da2ad <_RNvNtNtNtCs3js4z49Ikqb_4syom6engine5imdct7avx512k18ifft_stages_avx512+0x91d>
   d99ab:	4d 89 ca             	mov    r10,r9
   d99ae:	4d 85 c9             	test   r9,r9
   d99b1:	0f 84 56 0f 00 00    	je     da90d <_RNvNtNtNtCs3js4z49Ikqb_4syom6engine5imdct7avx512k18ifft_stages_avx512+0xf7d>
   d99b7:	48 8b 84 24 d8 00 00 	mov    rax,QWORD PTR [rsp+0xd8]
   d99be:	00 
   d99bf:	48 85 c0             	test   rax,rax
   d99c2:	0f 84 55 0f 00 00    	je     da91d <_RNvNtNtNtCs3js4z49Ikqb_4syom6engine5imdct7avx512k18ifft_stages_avx512+0xf8d>
   d99c8:	48 89 fb             	mov    rbx,rdi
   d99cb:	4c 8b bc 24 d0 00 00 	mov    r15,QWORD PTR [rsp+0xd0]
   d99d2:	00 
   d99d3:	48 89 74 24 08       	mov    QWORD PTR [rsp+0x8],rsi
   d99d8:	48 89 f7             	mov    rdi,rsi
   d99db:	48 d1 ef             	shr    rdi,1
   d99de:	48 89 ce             	mov    rsi,rcx
   d99e1:	48 d1 ee             	shr    rsi,1
   d99e4:	48 39 fe             	cmp    rsi,rdi
   d99e7:	48 0f 43 f7          	cmovae rsi,rdi
   d99eb:	48 85 f6             	test   rsi,rsi
   d99ee:	4c 89 44 24 10       	mov    QWORD PTR [rsp+0x10],r8
   d99f3:	0f 84 73 02 00 00    	je     d9c6c <_RNvNtNtNtCs3js4z49Ikqb_4syom6engine5imdct7avx512k18ifft_stages_avx512+0x2dc>
   d99f9:	c4 c1 7a 10 00       	vmovss xmm0,DWORD PTR [r8]
   d99fe:	c4 c1 7a 10 0f       	vmovss xmm1,DWORD PTR [r15]
   d9a03:	48 83 fe 04          	cmp    rsi,0x4
   d9a07:	73 07                	jae    d9a10 <_RNvNtNtNtCs3js4z49Ikqb_4syom6engine5imdct7avx512k18ifft_stages_avx512+0x80>
   d9a09:	31 ff                	xor    edi,edi
   d9a0b:	e9 00 02 00 00       	jmp    d9c10 <_RNvNtNtNtCs3js4z49Ikqb_4syom6engine5imdct7avx512k18ifft_stages_avx512+0x280>
   d9a10:	49 be f0 ff ff ff ff 	movabs r14,0xffffffffffffff0
   d9a17:	ff ff 0f 
   d9a1a:	48 83 fe 10          	cmp    rsi,0x10
   d9a1e:	73 07                	jae    d9a27 <_RNvNtNtNtCs3js4z49Ikqb_4syom6engine5imdct7avx512k18ifft_stages_avx512+0x97>
   d9a20:	31 ff                	xor    edi,edi
   d9a22:	e9 2e 01 00 00       	jmp    d9b55 <_RNvNtNtNtCs3js4z49Ikqb_4syom6engine5imdct7avx512k18ifft_stages_avx512+0x1c5>
   d9a27:	48 89 f7             	mov    rdi,rsi
   d9a2a:	4c 21 f7             	and    rdi,r14
   d9a2d:	62 f2 7d 48 18 d0    	vbroadcastss zmm2,xmm0
   d9a33:	62 f2 7d 48 18 d9    	vbroadcastss zmm3,xmm1
   d9a39:	4c 8d 0c f5 00 00 00 	lea    r9,[rsi*8+0x0]
   d9a40:	00 
   d9a41:	49 83 e1 80          	and    r9,0xffffffffffffff80
   d9a45:	45 31 db             	xor    r11d,r11d
   d9a48:	62 f1 7c 48 28 25 6e 	vmovaps zmm4,ZMMWORD PTR [rip+0xfffffffffff3076e]        # a1c0 <__abi_tag+0x9ec4>
   d9a4f:	07 f3 ff 
   d9a52:	62 f1 7c 48 28 2d a4 	vmovaps zmm5,ZMMWORD PTR [rip+0xfffffffffff307a4]        # a200 <__abi_tag+0x9f04>
   d9a59:	07 f3 ff 
   d9a5c:	62 f1 7c 48 28 35 da 	vmovaps zmm6,ZMMWORD PTR [rip+0xfffffffffff307da]        # a240 <__abi_tag+0x9f44>
   d9a63:	07 f3 ff 
   d9a66:	62 f1 7c 48 28 3d 10 	vmovaps zmm7,ZMMWORD PTR [rip+0xfffffffffff30810]        # a280 <__abi_tag+0x9f84>
   d9a6d:	08 f3 ff 
   d9a70:	62 31 7c 48 10 04 1b 	vmovups zmm8,ZMMWORD PTR [rbx+r11*1]
   d9a77:	62 31 7c 48 10 4c 1b 	vmovups zmm9,ZMMWORD PTR [rbx+r11*1+0x40]
   d9a7e:	01 
   d9a7f:	62 51 7c 48 28 d0    	vmovaps zmm10,zmm8
   d9a85:	62 52 5d 48 7f d1    	vpermt2ps zmm10,zmm4,zmm9
   d9a8b:	62 52 55 48 7f c1    	vpermt2ps zmm8,zmm5,zmm9
   d9a91:	62 31 7c 48 10 0c 1a 	vmovups zmm9,ZMMWORD PTR [rdx+r11*1]
   d9a98:	62 31 7c 48 10 5c 1a 	vmovups zmm11,ZMMWORD PTR [rdx+r11*1+0x40]
   d9a9f:	01 
   d9aa0:	62 51 7c 48 28 e1    	vmovaps zmm12,zmm9
   d9aa6:	62 52 5d 48 7f e3    	vpermt2ps zmm12,zmm4,zmm11
   d9aac:	62 52 55 48 7f cb    	vpermt2ps zmm9,zmm5,zmm11
   d9ab2:	62 51 6c 48 59 d8    	vmulps zmm11,zmm2,zmm8
   d9ab8:	62 51 64 48 59 e9    	vmulps zmm13,zmm3,zmm9
   d9abe:	62 51 24 48 5c dd    	vsubps zmm11,zmm11,zmm13
   d9ac4:	62 51 6c 48 59 c9    	vmulps zmm9,zmm2,zmm9
   d9aca:	62 51 64 48 59 c0    	vmulps zmm8,zmm3,zmm8
   d9ad0:	62 51 3c 48 58 c1    	vaddps zmm8,zmm8,zmm9
   d9ad6:	62 51 2c 48 5c cb    	vsubps zmm9,zmm10,zmm11
   d9adc:	62 51 1c 48 5c e8    	vsubps zmm13,zmm12,zmm8
   d9ae2:	62 51 2c 48 58 d3    	vaddps zmm10,zmm10,zmm11
   d9ae8:	62 51 7c 48 28 da    	vmovaps zmm11,zmm10
   d9aee:	62 52 4d 48 7f d9    	vpermt2ps zmm11,zmm6,zmm9
   d9af4:	62 52 45 48 7f d1    	vpermt2ps zmm10,zmm7,zmm9
   d9afa:	62 31 7c 48 11 14 1b 	vmovups ZMMWORD PTR [rbx+r11*1],zmm10
   d9b01:	62 31 7c 48 11 5c 1b 	vmovups ZMMWORD PTR [rbx+r11*1+0x40],zmm11
   d9b08:	01 
   d9b09:	62 51 1c 48 58 c0    	vaddps zmm8,zmm12,zmm8
   d9b0f:	62 51 7c 48 28 c8    	vmovaps zmm9,zmm8
   d9b15:	62 52 45 48 7f cd    	vpermt2ps zmm9,zmm7,zmm13
   d9b1b:	62 52 4d 48 7f c5    	vpermt2ps zmm8,zmm6,zmm13
   d9b21:	62 31 7c 48 11 44 1a 	vmovups ZMMWORD PTR [rdx+r11*1+0x40],zmm8
   d9b28:	01 
   d9b29:	62 31 7c 48 11 0c 1a 	vmovups ZMMWORD PTR [rdx+r11*1],zmm9
   d9b30:	49 83 eb 80          	sub    r11,0xffffffffffffff80
   d9b34:	4d 39 d9             	cmp    r9,r11
   d9b37:	0f 85 33 ff ff ff    	jne    d9a70 <_RNvNtNtNtCs3js4z49Ikqb_4syom6engine5imdct7avx512k18ifft_stages_avx512+0xe0>
   d9b3d:	48 39 fe             	cmp    rsi,rdi
   d9b40:	4c 8b 44 24 10       	mov    r8,QWORD PTR [rsp+0x10]
   d9b45:	0f 84 21 01 00 00    	je     d9c6c <_RNvNtNtNtCs3js4z49Ikqb_4syom6engine5imdct7avx512k18ifft_stages_avx512+0x2dc>
   d9b4b:	40 f6 c6 0c          	test   sil,0xc
   d9b4f:	0f 84 bb 00 00 00    	je     d9c10 <_RNvNtNtNtCs3js4z49Ikqb_4syom6engine5imdct7avx512k18ifft_stages_avx512+0x280>
   d9b55:	49 89 f9             	mov    r9,rdi
   d9b58:	49 83 c6 0c          	add    r14,0xc
   d9b5c:	4c 89 f7             	mov    rdi,r14
   d9b5f:	48 21 f7             	and    rdi,rsi
   d9b62:	c4 e2 79 18 d0       	vbroadcastss xmm2,xmm0
   d9b67:	c4 e2 79 18 d9       	vbroadcastss xmm3,xmm1
   d9b6c:	0f 1f 40 00          	nop    DWORD PTR [rax+0x0]
   d9b70:	c4 a1 78 10 24 cb    	vmovups xmm4,XMMWORD PTR [rbx+r9*8]
   d9b76:	c4 a1 78 10 6c cb 10 	vmovups xmm5,XMMWORD PTR [rbx+r9*8+0x10]
   d9b7d:	c5 d8 c6 f5 88       	vshufps xmm6,xmm4,xmm5,0x88
   d9b82:	c5 d8 c6 e5 dd       	vshufps xmm4,xmm4,xmm5,0xdd
   d9b87:	c4 a1 78 10 2c ca    	vmovups xmm5,XMMWORD PTR [rdx+r9*8]
   d9b8d:	c4 a1 78 10 7c ca 10 	vmovups xmm7,XMMWORD PTR [rdx+r9*8+0x10]
   d9b94:	c5 50 c6 c7 88       	vshufps xmm8,xmm5,xmm7,0x88
   d9b99:	c5 d0 c6 ef dd       	vshufps xmm5,xmm5,xmm7,0xdd
   d9b9e:	c5 e8 59 fc          	vmulps xmm7,xmm2,xmm4
   d9ba2:	c5 60 59 cd          	vmulps xmm9,xmm3,xmm5
   d9ba6:	c4 c1 40 5c f9       	vsubps xmm7,xmm7,xmm9
   d9bab:	c5 e8 59 ed          	vmulps xmm5,xmm2,xmm5
   d9baf:	c5 e0 59 e4          	vmulps xmm4,xmm3,xmm4
   d9bb3:	c5 d8 58 e5          	vaddps xmm4,xmm4,xmm5
   d9bb7:	c5 c8 5c ef          	vsubps xmm5,xmm6,xmm7
   d9bbb:	c5 38 5c cc          	vsubps xmm9,xmm8,xmm4
   d9bbf:	c5 c8 58 f7          	vaddps xmm6,xmm6,xmm7
   d9bc3:	c5 c8 15 fd          	vunpckhps xmm7,xmm6,xmm5
   d9bc7:	c5 c8 14 ed          	vunpcklps xmm5,xmm6,xmm5
   d9bcb:	c4 a1 78 11 2c cb    	vmovups XMMWORD PTR [rbx+r9*8],xmm5
   d9bd1:	c4 a1 78 11 7c cb 10 	vmovups XMMWORD PTR [rbx+r9*8+0x10],xmm7
   d9bd8:	c5 b8 58 e4          	vaddps xmm4,xmm8,xmm4
   d9bdc:	c4                   	.byte 0xc4
   d9bdd:	c1                   	.byte 0xc1
   d9bde:	58                   	pop    rax
   d9bdf:	14                   	.byte 0x14
