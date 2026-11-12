#pragma once

#include "CoreMinimal.h"
#include "Factories/Factory.h"
#include "WeaveStoryFactory.generated.h"

/** Imports .weave files by compiling them with weavec (env WEAVEC, else PATH, else ~/.cargo/bin). */
UCLASS()
class UWeaveStoryFactory : public UFactory
{
	GENERATED_BODY()

public:
	UWeaveStoryFactory();
	virtual UObject* FactoryCreateFile(UClass* InClass, UObject* InParent, FName InName, EObjectFlags Flags,
		const FString& Filename, const TCHAR* Parms, FFeedbackContext* Warn, bool& bOutOperationCanceled) override;

	/** Compile a .weave file to JSON. Returns false and fills OutError on failure. */
	static bool Compile(const FString& Filename, FString& OutJson, FString& OutError);
};

/** Creates an empty UWeaveStory (for scripts that set CompiledJson themselves). */
UCLASS()
class UWeaveStoryFactoryNew : public UFactory
{
	GENERATED_BODY()

public:
	UWeaveStoryFactoryNew();
	virtual UObject* FactoryCreateNew(UClass* InClass, UObject* InParent, FName InName, EObjectFlags Flags,
		UObject* Context, FFeedbackContext* Warn) override;
};
