#pragma once

#include "CoreMinimal.h"
#include "UObject/Object.h"
#include "WeaveStoryPlayer.generated.h"

class UWeaveStory;

UENUM(BlueprintType)
enum class EWeaveFrameKind : uint8
{
	Line,
	Choices,
	Ended,
	Error
};

USTRUCT(BlueprintType)
struct WEAVE_API FWeaveChoice
{
	GENERATED_BODY()

	/** Stable compiler-generated identifier. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString Id;

	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString Text;

	/** The choice disappears once selected. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	bool bOnce = false;
};

USTRUCT(BlueprintType)
struct WEAVE_API FWeaveFrame
{
	GENERATED_BODY()

	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	EWeaveFrameKind Kind = EWeaveFrameKind::Ended;

	/** The line, for Kind == Line. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString Text;

	/** The choices, for Kind == Choices. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	TArray<FWeaveChoice> Choices;

	/** Presentation cues (scene, show, speaker, transition...) as a JSON array, in source order. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString PresentationJson;

	/** Pattern draws as a JSON array. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString DrawsJson;

	/** The error, for Kind == Error. */
	UPROPERTY(BlueprintReadOnly, Category = "Weave")
	FString Error;
};

/** Plays a UWeaveStory with the shared Weave runtime: same semantics as the Bevy and browser hosts. */
UCLASS(BlueprintType)
class WEAVE_API UWeaveStoryPlayer : public UObject
{
	GENERATED_BODY()

public:
	/** Create a player. Returns null and sets Error on failure. */
	UFUNCTION(BlueprintCallable, Category = "Weave")
	static UWeaveStoryPlayer* CreateWeaveStoryPlayer(UWeaveStory* Story, int64 Seed, FString& Error);

	/** Advance to the next line, choice set or ending. */
	UFUNCTION(BlueprintCallable, Category = "Weave")
	FWeaveFrame Continue();

	UFUNCTION(BlueprintCallable, Category = "Weave")
	TArray<FWeaveChoice> GetChoices();

	/** Select a currently eligible choice by zero-based index. */
	UFUNCTION(BlueprintCallable, Category = "Weave")
	bool Choose(int32 Index, FString& Error);

	UFUNCTION(BlueprintPure, Category = "Weave")
	bool CanContinue() const;

	UFUNCTION(BlueprintCallable, Category = "Weave")
	bool Restart(int64 Seed, FString& Error);

	/** Versioned state JSON for save games. */
	UFUNCTION(BlueprintCallable, Category = "Weave")
	FString SaveState(FString& Error);

	UFUNCTION(BlueprintCallable, Category = "Weave")
	bool RestoreState(const FString& State, FString& Error);

	virtual void BeginDestroy() override;

private:
	void* Handle = nullptr;
};
