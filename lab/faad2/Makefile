# Isolated FAAD2 2.11.3 adapter. Not invoked by cargo test --workspace.
# make FAAD2_PREFIX=/path/to/prefix

FAAD2_PREFIX ?= $(error set FAAD2_PREFIX to a FAAD2 2.11.3 install)

CC ?= gcc
CFLAGS ?= -O2 -Wall -Wextra -std=c11
CPPFLAGS += -I$(FAAD2_PREFIX)/include
LDFLAGS += -L$(FAAD2_PREFIX)/lib
LDLIBS += -lfaad -lm

.PHONY: all clean

all: faad_driver

faad_adapt.o: faad_adapt.c faad_adapt.h
	$(CC) $(CFLAGS) $(CPPFLAGS) -c faad_adapt.c -o $@

faad_driver.o: faad_driver.c faad_adapt.h
	$(CC) $(CFLAGS) $(CPPFLAGS) -c faad_driver.c -o $@

faad_driver: faad_adapt.o faad_driver.o
	$(CC) $(CFLAGS) -o $@ faad_adapt.o faad_driver.o $(LDFLAGS) $(LDLIBS)

clean:
	rm -f faad_adapt.o faad_driver.o faad_driver
