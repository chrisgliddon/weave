#include "WeaveStoryFactory.h"

#include "HAL/PlatformMisc.h"
#include "HAL/PlatformProcess.h"
#include "Misc/Paths.h"
#include "Modules/ModuleManager.h"
#include "WeaveStory.h"

namespace
{
	FString FindWeavec()
	{
		const FString Env = FPlatformMisc::GetEnvironmentVariable(TEXT("WEAVEC"));
		if (!Env.IsEmpty())
		{
			return Env;
		}
#if PLATFORM_WINDOWS
		const TCHAR* Exe = TEXT("weavec.exe");
		const TCHAR* Sep = TEXT(";");
#else
		const TCHAR* Exe = TEXT("weavec");
		const TCHAR* Sep = TEXT(":");
#endif
		TArray<FString> Dirs;
		FPlatformMisc::GetEnvironmentVariable(TEXT("PATH")).ParseIntoArray(Dirs, Sep);
		Dirs.Add(FPaths::Combine(FPlatformMisc::GetEnvironmentVariable(TEXT("HOME")), TEXT(".cargo/bin")));
		for (const FString& Dir : Dirs)
		{
			const FString Candidate = FPaths::Combine(Dir, Exe);
			if (FPaths::FileExists(Candidate))
			{
				return Candidate;
			}
		}
		return Exe;
	}
}

UWeaveStoryFactory::UWeaveStoryFactory()
{
	SupportedClass = UWeaveStory::StaticClass();
	bCreateNew = false;
	bEditorImport = true;
	bText = false;
	Formats.Add(TEXT("weave;Weave story"));
}

bool UWeaveStoryFactory::Compile(const FString& Filename, FString& OutJson, FString& OutError)
{
	int32 Code = -1;
	FString StdErr;
	const FString Args = FString::Printf(TEXT("\"%s\" --format json --output -"), *FPaths::ConvertRelativePathToFull(Filename));
	if (!FPlatformProcess::ExecProcess(*FindWeavec(), *Args, &Code, &OutJson, &StdErr) || Code != 0)
	{
		OutError = FString::Printf(TEXT("weavec failed (%d): %s"), Code, *StdErr);
		return false;
	}
	return true;
}

UObject* UWeaveStoryFactory::FactoryCreateFile(UClass* InClass, UObject* InParent, FName InName, EObjectFlags Flags,
	const FString& Filename, const TCHAR* Parms, FFeedbackContext* Warn, bool& bOutOperationCanceled)
{
	FString Json, Error;
	if (!Compile(Filename, Json, Error))
	{
		Warn->Logf(ELogVerbosity::Error, TEXT("%s"), *Error);
		return nullptr;
	}
	UWeaveStory* Story = NewObject<UWeaveStory>(InParent, InClass, InName, Flags);
	Story->SetCompiledJson(Json, Filename);
	return Story;
}

UWeaveStoryFactoryNew::UWeaveStoryFactoryNew()
{
	SupportedClass = UWeaveStory::StaticClass();
	bCreateNew = true;
	bEditAfterNew = false;
}

UObject* UWeaveStoryFactoryNew::FactoryCreateNew(UClass* InClass, UObject* InParent, FName InName, EObjectFlags Flags,
	UObject* Context, FFeedbackContext* Warn)
{
	return NewObject<UWeaveStory>(InParent, InClass, InName, Flags);
}

IMPLEMENT_MODULE(FDefaultModuleImpl, WeaveEditor)
