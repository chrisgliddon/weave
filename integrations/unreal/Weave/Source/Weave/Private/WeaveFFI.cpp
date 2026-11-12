#include "WeaveFFI.h"

#include "HAL/PlatformProcess.h"
#include "Interfaces/IPluginManager.h"
#include "Misc/Paths.h"

namespace
{
	FWeaveFFI GWeave;
	bool GLoaded = false;

	FString LibraryPath()
	{
		const TSharedPtr<IPlugin> Plugin = IPluginManager::Get().FindPlugin(TEXT("Weave"));
		const FString Base = Plugin.IsValid() ? Plugin->GetBaseDir() : FPaths::ProjectPluginsDir() / TEXT("Weave");
		const FString Dir = Base / TEXT("Binaries/ThirdParty/Weave");
#if PLATFORM_LINUX
		return Dir / TEXT("Linux/libweave_ffi.so");
#elif PLATFORM_MAC
		return Dir / TEXT("Mac/libweave_ffi.dylib");
#elif PLATFORM_WINDOWS
		return Dir / TEXT("Win64/weave_ffi.dll");
#else
		return FString();
#endif
	}
}

bool FWeaveFFI::Load(FString& OutError)
{
	if (GLoaded)
	{
		return true;
	}
	const FString Path = FPaths::ConvertRelativePathToFull(LibraryPath());
	void* Lib = FPlatformProcess::GetDllHandle(*Path);
	if (!Lib)
	{
		OutError = FString::Printf(TEXT("Weave runtime library not found at %s (run integrations/unreal/stage-ffi.sh)"), *Path);
		return false;
	}
	auto Sym = [Lib](const TCHAR* Name) { return FPlatformProcess::GetDllExport(Lib, Name); };
	FWeaveFFI F;
	F.Handle = Lib;
	F.New = (FNew)Sym(TEXT("weave_player_new"));
	F.PlayerFree = (FFree)Sym(TEXT("weave_player_free"));
	F.StringFree = (FStrFree)Sym(TEXT("weave_string_free"));
	F.Advance = (FCall)Sym(TEXT("weave_player_advance"));
	F.Choices = (FCall)Sym(TEXT("weave_player_choices"));
	F.Choose = (FChoose)Sym(TEXT("weave_player_choose"));
	F.CanContinue = (FCanContinue)Sym(TEXT("weave_player_can_continue"));
	F.Restart = (FRestart)Sym(TEXT("weave_player_restart"));
	F.Save = (FCall)Sym(TEXT("weave_player_save"));
	F.Restore = (FRestore)Sym(TEXT("weave_player_restore"));
	F.AbiVersion = (FAbi)Sym(TEXT("weave_ffi_abi_version"));
	F.StringLookup = (FStringLookup)Sym(TEXT("weave_player_string"));
	F.Channels = (FCall)Sym(TEXT("weave_player_channels"));
	if (F.AbiVersion && F.AbiVersion() != ExpectedAbi)
	{
		OutError = FString::Printf(TEXT("Weave runtime ABI %u, plugin expects %u (rebuild and restage the runtime library)"),
			F.AbiVersion(), ExpectedAbi);
		return false;
	}
	if (!F.New || !F.PlayerFree || !F.StringFree || !F.Advance || !F.Choices || !F.Choose || !F.CanContinue
		|| !F.Restart || !F.Save || !F.Restore || !F.AbiVersion || !F.StringLookup || !F.Channels)
	{
		OutError = FString::Printf(TEXT("Weave runtime library at %s is missing exports"), *Path);
		return false;
	}
	GWeave = F;
	GLoaded = true;
	return true;
}

const FWeaveFFI& FWeaveFFI::Get()
{
	return GWeave;
}

FString FWeaveFFI::Take(char* Raw) const
{
	if (!Raw)
	{
		return TEXT("{\"ok\":false,\"error\":\"weave-ffi returned null\"}");
	}
	FString Out = FString(UTF8_TO_TCHAR(Raw));
	StringFree(Raw);
	return Out;
}
