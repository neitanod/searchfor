ifeq ($(OS),Windows_NT)
release test: deps
	@echo needle

# within recipe gap
	echo needle again
else
NAME := needle
endif
define BODY
needle body
endef
.RECIPEPREFIX = >
%.o: %.c
>echo needle pattern
