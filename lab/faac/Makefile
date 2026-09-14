# Isolated FAAC 1.31.1 encoder adapter.
# Not invoked by cargo test
# FAAC_PREFIX must contain include/faac.h and lib/libfaac.a (see PIN.md).

FAAC_PREFIX ?= $(error set FAAC_PREFIX to a FAAC 1.31.1 include+lib tree)

CC ?= gcc
CFLAGS ?= -O2 -Wall -Wextra -std=c11
CPPFLAGS += -I$(FAAC_PREFIX)/include -I.
LDFLAGS += -L$(FAAC_PREFIX)/lib
LDLIBS += -lfaac -lm

.PHONY: all clean

all: faac_driver

faac_adapt.o: faac_adapt.c faac_adapt.h
	$(CC) $(CFLAGS) $(CPPFLAGS) -c faac_adapt.c -o $@

faac_driver.o: faac_driver.c faac_adapt.h
	$(CC) $(CFLAGS) $(CPPFLAGS) -c faac_driver.c -o $@

faac_driver: faac_adapt.o faac_driver.o
	$(CC) $(CFLAGS) -o $@ faac_adapt.o faac_driver.o $(LDFLAGS) $(LDLIBS)

clean:
	rm -f faac_adapt.o faac_driver.o faac_driver
