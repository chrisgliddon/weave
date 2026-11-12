#pragma once

#include "CoreMinimal.h"

/** Thin loader for the weave-ffi C ABI. Every call that can fail returns a JSON envelope. */
struct WEAVE_API FWeaveFFI
{
	using FNew = char* (*)(const char*, uint64, void**);
	using FFree = void (*)(void*);
	using FStrFree = void (*)(char*);
	using FCall = char* (*)(void*);
	using FChoose = char* (*)(void*, SIZE_T);
	using FCanContinue = bool (*)(void*);
	using FRestart = char* (*)(void*, uint64);
	using FRestore = char* (*)(void*, const char*);
	using FAbi = uint32 (*)();
	using FStringLookup = char* (*)(void*, const char*, const char*);

	static constexpr uint32 ExpectedAbi = 2;

	/** Load the library once. Safe to call repeatedly. */
	static bool Load(FString& OutError);
	static const FWeaveFFI& Get();

	/** Call and take ownership of a returned string. */
	FString Take(char* Raw) const;

	FNew New = nullptr;
	FFree PlayerFree = nullptr;
	FStrFree StringFree = nullptr;
	FCall Advance = nullptr;
	FCall Choices = nullptr;
	FChoose Choose = nullptr;
	FCanContinue CanContinue = nullptr;
	FRestart Restart = nullptr;
	FCall Save = nullptr;
	FRestore Restore = nullptr;
	FAbi AbiVersion = nullptr;
	FStringLookup StringLookup = nullptr;
	FCall Channels = nullptr;
	void* Handle = nullptr;
};
