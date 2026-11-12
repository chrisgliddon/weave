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

	/** UTF-16 index into Text of the character at a Unicode-scalar offset (the runtime counts scalars). */
	int32 ScalarToUtf16(const FString& Text, int64 Scalar)
	{
		int32 Index = 0;
		for (int64 Seen = 0; Seen < Scalar && Index < Text.Len(); ++Seen)
		{
			const TCHAR Unit = Text[Index];
			const bool bPair = Unit >= 0xD800 && Unit <= 0xDBFF && Index + 1 < Text.Len();
			Index += bPair ? 2 : 1;
		}
		return Index;
	}

	FString ValueToString(const TSharedPtr<FJsonValue>& Value)
	{
		if (!Value.IsValid())
		{
			return FString();
		}
		switch (Value->Type)
		{
		case EJson::String:
			return Value->AsString();
		case EJson::Boolean:
			return Value->AsBool() ? TEXT("true") : TEXT("false");
		case EJson::Number:
			return FString::SanitizeFloat(Value->AsNumber(), 0);
		default:
			return FString();
		}
	}

	TArray<FString> ReadTags(const TSharedPtr<FJsonObject>& Obj)
	{
		TArray<FString> Out;
		const TArray<TSharedPtr<FJsonValue>>* Arr = nullptr;
		if (Obj.IsValid() && Obj->TryGetArrayField(TEXT("tags"), Arr))
		{
			for (const TSharedPtr<FJsonValue>& Value : *Arr)
			{
				Out.Add(ValueToString(Value));
			}
		}
		return Out;
	}

	TMap<FString, FString> ReadMeta(const TSharedPtr<FJsonObject>& Obj)
	{
		TMap<FString, FString> Out;
		const TSharedPtr<FJsonObject>* Meta = nullptr;
		if (Obj.IsValid() && Obj->TryGetObjectField(TEXT("meta"), Meta) && Meta && Meta->IsValid())
		{
			for (const auto& Pair : (*Meta)->Values)
			{
				Out.Add(FString(*Pair.Key), ValueToString(Pair.Value));
			}
		}
		return Out;
	}

	TArray<FWeaveStyleSpan> ReadSpans(const TSharedPtr<FJsonObject>& Obj, const FString& Text)
	{
		TArray<FWeaveStyleSpan> Out;
		const TArray<TSharedPtr<FJsonValue>>* Arr = nullptr;
		if (Obj.IsValid() && Obj->TryGetArrayField(TEXT("spans"), Arr))
		{
			for (const TSharedPtr<FJsonValue>& Value : *Arr)
			{
				const TSharedPtr<FJsonObject> Item = Value->AsObject();
				if (!Item.IsValid())
				{
					continue;
				}
				FWeaveStyleSpan Span;
				Span.Style = Item->GetStringField(TEXT("style"));
				Span.From = ScalarToUtf16(Text, static_cast<int64>(Item->GetNumberField(TEXT("from"))));
				Span.To = ScalarToUtf16(Text, static_cast<int64>(Item->GetNumberField(TEXT("to"))));
				Out.Add(Span);
			}
		}
		return Out;
	}

	TArray<FWeaveBeat> ReadBeats(const TSharedPtr<FJsonObject>& Obj, const FString& Text)
	{
		TArray<FWeaveBeat> Out;
		const TArray<TSharedPtr<FJsonValue>>* Arr = nullptr;
		if (Obj.IsValid() && Obj->TryGetArrayField(TEXT("beats"), Arr))
		{
			for (const TSharedPtr<FJsonValue>& Value : *Arr)
			{
				const TSharedPtr<FJsonObject> Item = Value->AsObject();
				if (!Item.IsValid())
				{
					continue;
				}
				FWeaveBeat Beat;
				Beat.At = ScalarToUtf16(Text, static_cast<int64>(Item->GetNumberField(TEXT("at"))));
				Beat.Seconds = static_cast<float>(Item->GetNumberField(TEXT("pause")));
				Out.Add(Beat);
			}
		}
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
				if (!C.IsValid())
				{
					continue;
				}
				FWeaveChoice Choice;
				Choice.Id = C->GetStringField(TEXT("id"));
				Choice.Text = C->GetStringField(TEXT("text"));
				Choice.bOnce = C->GetBoolField(TEXT("once"));
				Choice.Tags = ReadTags(C);
				Choice.Meta = ReadMeta(C);
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
		Frame.Id = Body->GetStringField(TEXT("id"));
		Frame.Channel = Body->GetStringField(TEXT("channel"));
		Body->TryGetStringField(TEXT("speaker"), Frame.Speaker);
		Frame.RichText = Body->GetStringField(TEXT("rich"));
		Frame.Tags = ReadTags(Body);
		Frame.Meta = ReadMeta(Body);
		Frame.Spans = ReadSpans(Body, Frame.Text);
		Frame.Beats = ReadBeats(Body, Frame.Text);
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

FWeaveString UWeaveStoryPlayer::GetString(const FString& Table, const FString& Key, FString& Error)
{
	FWeaveString Out;
	if (!Handle)
	{
		Error = TEXT("player is not loaded");
		return Out;
	}
	const FWeaveFFI& F = FWeaveFFI::Get();
	const FTCHARToUTF8 TableUtf8(*Table);
	const FTCHARToUTF8 KeyUtf8(*Key);
	const TSharedPtr<FJsonObject> Obj = Parse(F.Take(F.StringLookup(Handle, TableUtf8.Get(), KeyUtf8.Get())));
	const TSharedPtr<FJsonObject>* Entry = nullptr;
	if (!Ok(Obj, Error) || !Obj->TryGetObjectField(TEXT("string"), Entry) || !Entry || !Entry->IsValid())
	{
		if (Error.IsEmpty())
		{
			Error = TEXT("invalid string response from Weave runtime");
		}
		return Out;
	}
	Out.Text = (*Entry)->GetStringField(TEXT("text"));
	Out.RichText = (*Entry)->GetStringField(TEXT("rich"));
	Out.Spans = ReadSpans(*Entry, Out.Text);
	Out.Beats = ReadBeats(*Entry, Out.Text);
	Out.Tags = ReadTags(*Entry);
	Out.Meta = ReadMeta(*Entry);
	return Out;
}

TMap<FString, FWeaveChannel> UWeaveStoryPlayer::GetChannels()
{
	TMap<FString, FWeaveChannel> Out;
	if (!Handle)
	{
		return Out;
	}
	const FWeaveFFI& F = FWeaveFFI::Get();
	const TSharedPtr<FJsonObject> Obj = Parse(F.Take(F.Channels(Handle)));
	FString Error;
	const TSharedPtr<FJsonObject>* Map = nullptr;
	if (!Ok(Obj, Error) || !Obj->TryGetObjectField(TEXT("channels"), Map) || !Map || !Map->IsValid())
	{
		return Out;
	}
	for (const auto& Pair : (*Map)->Values)
	{
		const TSharedPtr<FJsonObject> Item = Pair.Value.IsValid() ? Pair.Value->AsObject() : nullptr;
		if (!Item.IsValid())
		{
			continue;
		}
		FWeaveChannel Channel;
		Item->TryGetBoolField(TEXT("animated"), Channel.bAnimated);
		double Cps = 0.0;
		if (Item->TryGetNumberField(TEXT("cps"), Cps))
		{
			Channel.bHasCharactersPerSecond = true;
			Channel.CharactersPerSecond = static_cast<float>(Cps);
		}
		Channel.Meta = ReadMeta(Item);
		Out.Add(FString(*Pair.Key), Channel);
	}
	return Out;
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
