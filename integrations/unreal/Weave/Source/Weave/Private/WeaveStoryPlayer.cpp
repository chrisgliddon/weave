#include "WeaveStoryPlayer.h"

#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Serialization/JsonWriter.h"
#include "UObject/Package.h"
#include "WeaveFFI.h"
#include "WeaveStory.h"

namespace
{
	TSharedPtr<FJsonObject> Parse(const FString& Text)
	{
		TSharedPtr<FJsonObject> Obj;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Obj);
		return Obj;
	}

	bool Ok(const TSharedPtr<FJsonObject>& Obj, FString& Error)
	{
		if (!Obj.IsValid())
		{
			Error = TEXT("invalid response from Weave runtime");
			return false;
		}
		if (!Obj->GetBoolField(TEXT("ok")))
		{
			Error = Obj->GetStringField(TEXT("error"));
			return false;
		}
		return true;
	}

	FString ArrayToString(const TSharedPtr<FJsonObject>& Obj, const TCHAR* Field)
	{
		const TArray<TSharedPtr<FJsonValue>>* Arr = nullptr;
		if (!Obj.IsValid() || !Obj->TryGetArrayField(Field, Arr))
		{
			return TEXT("[]");
		}
		FString Out;
		const TSharedRef<TJsonWriter<>> Writer = TJsonWriterFactory<>::Create(&Out);
		FJsonSerializer::Serialize(*Arr, Writer);
		return Out;
	}

	TArray<FWeaveChoice> ReadChoices(const TSharedPtr<FJsonObject>& Obj)
	{
		TArray<FWeaveChoice> Out;
		const TArray<TSharedPtr<FJsonValue>>* Arr = nullptr;
		if (Obj.IsValid() && Obj->TryGetArrayField(TEXT("choices"), Arr))
		{
			for (const TSharedPtr<FJsonValue>& V : *Arr)
			{
				const TSharedPtr<FJsonObject> C = V->AsObject();
				FWeaveChoice Choice;
				Choice.Id = C->GetStringField(TEXT("id"));
				Choice.Text = C->GetStringField(TEXT("text"));
				Choice.bOnce = C->GetBoolField(TEXT("once"));
				Out.Add(Choice);
			}
		}
		return Out;
	}
}

UWeaveStoryPlayer* UWeaveStoryPlayer::CreateWeaveStoryPlayer(UWeaveStory* Story, int64 Seed, FString& Error)
{
	if (!Story || Story->CompiledJson.IsEmpty())
	{
		Error = TEXT("story is null or has no compiled JSON");
		return nullptr;
	}
	if (!FWeaveFFI::Load(Error))
	{
		return nullptr;
	}
	const FWeaveFFI& F = FWeaveFFI::Get();
	void* Handle = nullptr;
	const FTCHARToUTF8 Utf8(*Story->CompiledJson);
	if (!Ok(Parse(F.Take(F.New(Utf8.Get(), static_cast<uint64>(Seed), &Handle))), Error))
	{
		return nullptr;
	}
	UWeaveStoryPlayer* Player = NewObject<UWeaveStoryPlayer>(GetTransientPackage());
	Player->Handle = Handle;
	return Player;
}

FWeaveFrame UWeaveStoryPlayer::Continue()
{
	FWeaveFrame Frame;
	if (!Handle)
	{
		Frame.Kind = EWeaveFrameKind::Error;
		Frame.Error = TEXT("player is not loaded");
		return Frame;
	}
	const FWeaveFFI& F = FWeaveFFI::Get();
	const TSharedPtr<FJsonObject> Obj = Parse(F.Take(F.Advance(Handle)));
	if (!Ok(Obj, Frame.Error))
	{
		Frame.Kind = EWeaveFrameKind::Error;
		Frame.PresentationJson = ArrayToString(Obj, TEXT("presentation"));
		return Frame;
	}
	const TSharedPtr<FJsonObject> Body = Obj->GetObjectField(TEXT("frame"));
	const FString Kind = Body->GetStringField(TEXT("kind"));
	Frame.PresentationJson = ArrayToString(Body, TEXT("presentation"));
	Frame.DrawsJson = ArrayToString(Body, TEXT("draws"));
	if (Kind == TEXT("line"))
	{
		Frame.Kind = EWeaveFrameKind::Line;
		Frame.Text = Body->GetStringField(TEXT("text"));
	}
	else if (Kind == TEXT("choices"))
	{
		Frame.Kind = EWeaveFrameKind::Choices;
		Frame.Choices = ReadChoices(Body);
	}
	else
	{
		Frame.Kind = EWeaveFrameKind::Ended;
	}
	return Frame;
}

TArray<FWeaveChoice> UWeaveStoryPlayer::GetChoices()
{
	if (!Handle)
	{
		return {};
	}
	const FWeaveFFI& F = FWeaveFFI::Get();
	return ReadChoices(Parse(F.Take(F.Choices(Handle))));
}

bool UWeaveStoryPlayer::Choose(int32 Index, FString& Error)
{
	if (!Handle || Index < 0)
	{
		Error = TEXT("player is not loaded or index is negative");
		return false;
	}
	const FWeaveFFI& F = FWeaveFFI::Get();
	return Ok(Parse(F.Take(F.Choose(Handle, static_cast<SIZE_T>(Index)))), Error);
}

bool UWeaveStoryPlayer::CanContinue() const
{
	return Handle && FWeaveFFI::Get().CanContinue(Handle);
}

bool UWeaveStoryPlayer::Restart(int64 Seed, FString& Error)
{
	if (!Handle)
	{
		Error = TEXT("player is not loaded");
		return false;
	}
	const FWeaveFFI& F = FWeaveFFI::Get();
	return Ok(Parse(F.Take(F.Restart(Handle, static_cast<uint64>(Seed)))), Error);
}

FString UWeaveStoryPlayer::SaveState(FString& Error)
{
	if (!Handle)
	{
		Error = TEXT("player is not loaded");
		return FString();
	}
	const FWeaveFFI& F = FWeaveFFI::Get();
	const TSharedPtr<FJsonObject> Obj = Parse(F.Take(F.Save(Handle)));
	return Ok(Obj, Error) ? Obj->GetStringField(TEXT("state")) : FString();
}

bool UWeaveStoryPlayer::RestoreState(const FString& State, FString& Error)
{
	if (!Handle)
	{
		Error = TEXT("player is not loaded");
		return false;
	}
	const FWeaveFFI& F = FWeaveFFI::Get();
	const FTCHARToUTF8 Utf8(*State);
	return Ok(Parse(F.Take(F.Restore(Handle, Utf8.Get()))), Error);
}

void UWeaveStoryPlayer::BeginDestroy()
{
	if (Handle)
	{
		FWeaveFFI::Get().PlayerFree(Handle);
		Handle = nullptr;
	}
	Super::BeginDestroy();
}
