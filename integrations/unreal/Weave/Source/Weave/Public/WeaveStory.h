#pragma once

#include "CoreMinimal.h"
#include "UObject/Object.h"
#include "WeaveStory.generated.h"

/** A compiled Weave story (weavec --format json). Import a .weave file, or set CompiledJson from script. */
UCLASS(BlueprintType)
class WEAVE_API UWeaveStory : public UObject
{
	GENERATED_BODY()

public:
	/** Compiled story IR as JSON. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Weave")
	FString CompiledJson;

	/** The .weave file this was compiled from, for reimport. */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Weave")
	FString SourceFile;

	UFUNCTION(BlueprintCallable, Category = "Weave")
	void SetCompiledJson(const FString& Json, const FString& Source)
	{
		Modify();
		CompiledJson = Json;
		SourceFile = Source;
	}
};
