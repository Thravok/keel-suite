/*
 * Keel Audio Virtual HAL — Core Audio AudioServerPlugIn.
 *
 * Speaks the same POSIX shm ring as crates/keel-audio-hal (keel_audio_hal_v1):
 *   header (64) + DeviceSlot[16] (80 each) + f32 rings.
 *
 * Install: packaging/macos/install-hal.sh
 */

#include <CoreAudio/AudioServerPlugIn.h>
#include <CoreFoundation/CoreFoundation.h>
#include <mach/mach_time.h>
#include <pthread.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>

#define KEEL_DRIVER_NAME "Keel Audio Virtual"
#define KEEL_MANUFACTURER "Keel"
#define KEEL_SHM_NAME "/keel_audio_hal_v1"
#define KEEL_MAGIC 0x4B4C4841u /* KLHA */
#define KEEL_MAX_DEVICES 16
#define KEEL_CHANNELS 2
#define KEEL_RING_FRAMES 48000
#define KEEL_HEADER_SIZE 64
#define KEEL_SLOT_SIZE 80
#define KEEL_SAMPLE_RATE 48000.0

enum { kKeelDevice_Count = 14 };

static const char *kDeviceNames[kKeelDevice_Count] = {
    "Keel Mix Personal", "Keel Mix Chat",   "Keel Mix Stream", "Keel Mix 4",
    "Keel Mix 5",        "Keel MicFX",      "Keel Game",       "Keel Music",
    "Keel Browser",      "Keel Voice Chat", "Keel SFX",        "Keel System",
    "Keel Aux 1",        "Keel Aux 2"};

typedef struct {
  uint32_t magic;
  uint32_t version;
  uint32_t sample_rate;
  uint32_t device_count;
  uint32_t write_idx;
  uint32_t read_idx;
  uint32_t frame_capacity;
  uint32_t channels;
  uint32_t pad[8];
} KeelShmHeader;

typedef struct {
  uint32_t id;
  uint32_t flags;
  uint32_t write_idx;
  uint32_t read_idx;
  char name[64];
} KeelDeviceSlot;

void *KeelAudio_Create(CFAllocatorRef allocator, CFUUIDRef type_uuid);

static AudioServerPlugInDriverInterface gInterface;
static AudioServerPlugInDriverInterface *gInterfacePtr = &gInterface;
static AudioServerPlugInDriverRef gDriverRef =
    (AudioServerPlugInDriverRef)&gInterfacePtr;

static void *gShm = NULL;
static size_t gShmSize = 0;
static bool gIORunning = false;
static uint64_t gHostTicksPerFrame = 0;
static uint64_t gNumberTimeStamps = 0;
static Float64 gSampleTime = 0.0;

static size_t keel_shm_size(void) {
  return KEEL_HEADER_SIZE + KEEL_SLOT_SIZE * KEEL_MAX_DEVICES +
         KEEL_MAX_DEVICES * KEEL_CHANNELS * KEEL_RING_FRAMES * sizeof(float);
}

static void keel_shm_open(void) {
  if (gShm)
    return;
  gShmSize = keel_shm_size();
  int fd = shm_open(KEEL_SHM_NAME, O_RDWR | O_CREAT, 0666);
  if (fd < 0)
    return;
  if (ftruncate(fd, (off_t)gShmSize) != 0) {
    close(fd);
    return;
  }
  void *p = mmap(NULL, gShmSize, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
  close(fd);
  if (p == MAP_FAILED)
    return;
  gShm = p;
  KeelShmHeader *h = (KeelShmHeader *)gShm;
  if (h->magic != KEEL_MAGIC) {
    memset(gShm, 0, gShmSize);
    h->magic = KEEL_MAGIC;
    h->version = 1;
    h->sample_rate = (uint32_t)KEEL_SAMPLE_RATE;
    h->device_count = kKeelDevice_Count;
    h->frame_capacity = KEEL_RING_FRAMES;
    h->channels = KEEL_CHANNELS;
    KeelDeviceSlot *slots =
        (KeelDeviceSlot *)((uint8_t *)gShm + KEEL_HEADER_SIZE);
    for (int i = 0; i < kKeelDevice_Count; i++) {
      slots[i].id = (uint32_t)(i + 1);
      slots[i].flags = 0x7;
      strncpy(slots[i].name, kDeviceNames[i], 63);
    }
  }
}

static float *keel_ring(int device_index) {
  if (!gShm || device_index < 0 || device_index >= KEEL_MAX_DEVICES)
    return NULL;
  uint8_t *base = (uint8_t *)gShm + KEEL_HEADER_SIZE +
                  KEEL_SLOT_SIZE * KEEL_MAX_DEVICES;
  return (float *)(base +
                   (size_t)device_index * KEEL_CHANNELS * KEEL_RING_FRAMES *
                       sizeof(float));
}

static KeelDeviceSlot *keel_slot(int device_index) {
  if (!gShm || device_index < 0 || device_index >= KEEL_MAX_DEVICES)
    return NULL;
  return (KeelDeviceSlot *)((uint8_t *)gShm + KEEL_HEADER_SIZE) + device_index;
}

static ULONG Keel_AddRef(void *self) {
  (void)self;
  return 1;
}
static ULONG Keel_Release(void *self) {
  (void)self;
  return 1;
}
static HRESULT Keel_QueryInterface(void *self, REFIID iid, LPVOID *ppv) {
  (void)self;
  (void)iid;
  if (!ppv)
    return kAudioHardwareIllegalOperationError;
  *ppv = gDriverRef;
  return 0;
}

static OSStatus Keel_Initialize(AudioServerPlugInDriverRef inDriver,
                                AudioServerPlugInHostRef inHost) {
  (void)inDriver;
  (void)inHost;
  keel_shm_open();
  mach_timebase_info_data_t info;
  mach_timebase_info(&info);
  double nanosPerHost = (double)info.numer / (double)info.denom;
  gHostTicksPerFrame =
      (uint64_t)((1.0e9 / KEEL_SAMPLE_RATE) / nanosPerHost);
  return noErr;
}

static Boolean Keel_HasProperty(AudioServerPlugInDriverRef inDriver,
                                AudioObjectID inObjectID,
                                pid_t inClientProcessID,
                                const AudioObjectPropertyAddress *inAddress) {
  (void)inDriver;
  (void)inObjectID;
  (void)inClientProcessID;
  if (!inAddress)
    return false;
  switch (inAddress->mSelector) {
  case kAudioObjectPropertyManufacturer:
  case kAudioObjectPropertyName:
  case kAudioPlugInPropertyDeviceList:
  case kAudioDevicePropertyDeviceUID:
  case kAudioDevicePropertyModelUID:
  case kAudioDevicePropertyTransportType:
  case kAudioDevicePropertyNominalSampleRate:
  case kAudioDevicePropertyAvailableNominalSampleRates:
  case kAudioDevicePropertyIsHidden:
  case kAudioDevicePropertyStreams:
  case kAudioStreamPropertyDirection:
  case kAudioStreamPropertyVirtualFormat:
  case kAudioStreamPropertyPhysicalFormat:
    return true;
  default:
    return false;
  }
}

static OSStatus Keel_IsPropertySettable(AudioServerPlugInDriverRef inDriver,
                                        AudioObjectID inObjectID,
                                        pid_t inClientProcessID,
                                        const AudioObjectPropertyAddress *inAddress,
                                        Boolean *outIsSettable) {
  (void)inDriver;
  (void)inObjectID;
  (void)inClientProcessID;
  (void)inAddress;
  if (outIsSettable)
    *outIsSettable = false;
  return noErr;
}

static OSStatus Keel_GetPropertyDataSize(AudioServerPlugInDriverRef inDriver,
                                         AudioObjectID inObjectID,
                                         pid_t inClientProcessID,
                                         const AudioObjectPropertyAddress *inAddress,
                                         UInt32 inQualifierDataSize,
                                         const void *inQualifierData,
                                         UInt32 *outDataSize) {
  (void)inDriver;
  (void)inObjectID;
  (void)inClientProcessID;
  (void)inQualifierDataSize;
  (void)inQualifierData;
  if (!inAddress || !outDataSize)
    return kAudioHardwareIllegalOperationError;
  switch (inAddress->mSelector) {
  case kAudioObjectPropertyManufacturer:
  case kAudioObjectPropertyName:
  case kAudioDevicePropertyDeviceUID:
  case kAudioDevicePropertyModelUID:
    *outDataSize = sizeof(CFStringRef);
    return noErr;
  case kAudioPlugInPropertyDeviceList:
    *outDataSize = sizeof(AudioObjectID) * kKeelDevice_Count;
    return noErr;
  case kAudioDevicePropertyTransportType:
  case kAudioDevicePropertyIsHidden:
    *outDataSize = sizeof(UInt32);
    return noErr;
  case kAudioDevicePropertyNominalSampleRate:
    *outDataSize = sizeof(Float64);
    return noErr;
  case kAudioDevicePropertyStreams:
    *outDataSize = sizeof(AudioObjectID) * 2;
    return noErr;
  case kAudioStreamPropertyDirection:
    *outDataSize = sizeof(UInt32);
    return noErr;
  case kAudioStreamPropertyVirtualFormat:
  case kAudioStreamPropertyPhysicalFormat:
    *outDataSize = sizeof(AudioStreamBasicDescription);
    return noErr;
  default:
    return kAudioHardwareUnknownPropertyError;
  }
}

static OSStatus Keel_GetPropertyData(AudioServerPlugInDriverRef inDriver,
                                     AudioObjectID inObjectID,
                                     pid_t inClientProcessID,
                                     const AudioObjectPropertyAddress *inAddress,
                                     UInt32 inQualifierDataSize,
                                     const void *inQualifierData,
                                     UInt32 inDataSize, UInt32 *outDataSize,
                                     void *outData) {
  (void)inDriver;
  (void)inClientProcessID;
  (void)inQualifierDataSize;
  (void)inQualifierData;
  if (!inAddress || !outData || !outDataSize)
    return kAudioHardwareIllegalOperationError;

  if (inAddress->mSelector == kAudioPlugInPropertyDeviceList) {
    AudioObjectID *ids = (AudioObjectID *)outData;
    UInt32 n = inDataSize / sizeof(AudioObjectID);
    if (n > kKeelDevice_Count)
      n = kKeelDevice_Count;
    for (UInt32 i = 0; i < n; i++)
      ids[i] = 1000 + i;
    *outDataSize = n * sizeof(AudioObjectID);
    return noErr;
  }

  if (inAddress->mSelector == kAudioObjectPropertyManufacturer) {
    *(CFStringRef *)outData =
        CFStringCreateWithCString(NULL, KEEL_MANUFACTURER, kCFStringEncodingUTF8);
    *outDataSize = sizeof(CFStringRef);
    return noErr;
  }

  int devIndex = (int)inObjectID - 1000;
  if (devIndex < 0 || devIndex >= kKeelDevice_Count)
    devIndex = 0;

  switch (inAddress->mSelector) {
  case kAudioObjectPropertyName:
  case kAudioDevicePropertyDeviceUID: {
    char uid[128];
    snprintf(uid, sizeof(uid), "KeelAudioVirtual.%d", devIndex);
    const char *s =
        (inAddress->mSelector == kAudioObjectPropertyName) ? kDeviceNames[devIndex]
                                                           : uid;
    *(CFStringRef *)outData =
        CFStringCreateWithCString(NULL, s, kCFStringEncodingUTF8);
    *outDataSize = sizeof(CFStringRef);
    return noErr;
  }
  case kAudioDevicePropertyModelUID:
    *(CFStringRef *)outData =
        CFStringCreateWithCString(NULL, "KeelAudioVirtual", kCFStringEncodingUTF8);
    *outDataSize = sizeof(CFStringRef);
    return noErr;
  case kAudioDevicePropertyTransportType:
    *(UInt32 *)outData = kAudioDeviceTransportTypeVirtual;
    *outDataSize = sizeof(UInt32);
    return noErr;
  case kAudioDevicePropertyIsHidden:
    *(UInt32 *)outData = 0;
    *outDataSize = sizeof(UInt32);
    return noErr;
  case kAudioDevicePropertyNominalSampleRate:
    *(Float64 *)outData = KEEL_SAMPLE_RATE;
    *outDataSize = sizeof(Float64);
    return noErr;
  case kAudioDevicePropertyStreams: {
    AudioObjectID *ids = (AudioObjectID *)outData;
    ids[0] = 2000 + (AudioObjectID)devIndex; /* output */
    ids[1] = 3000 + (AudioObjectID)devIndex; /* input */
    *outDataSize = 2 * sizeof(AudioObjectID);
    return noErr;
  }
  case kAudioStreamPropertyDirection: {
    /* 2000+ = out (0), 3000+ = in (1) */
    *(UInt32 *)outData = (inObjectID >= 3000) ? 1 : 0;
    *outDataSize = sizeof(UInt32);
    return noErr;
  }
  case kAudioStreamPropertyVirtualFormat:
  case kAudioStreamPropertyPhysicalFormat: {
    AudioStreamBasicDescription *asbd = (AudioStreamBasicDescription *)outData;
    memset(asbd, 0, sizeof(*asbd));
    asbd->mSampleRate = KEEL_SAMPLE_RATE;
    asbd->mFormatID = kAudioFormatLinearPCM;
    asbd->mFormatFlags =
        kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked | kAudioFormatFlagIsNonInterleaved;
    asbd->mBytesPerPacket = 4;
    asbd->mFramesPerPacket = 1;
    asbd->mBytesPerFrame = 4;
    asbd->mChannelsPerFrame = KEEL_CHANNELS;
    asbd->mBitsPerChannel = 32;
    *outDataSize = sizeof(*asbd);
    return noErr;
  }
  default:
    return kAudioHardwareUnknownPropertyError;
  }
}

static OSStatus Keel_SetPropertyData(AudioServerPlugInDriverRef inDriver,
                                     AudioObjectID inObjectID,
                                     pid_t inClientProcessID,
                                     const AudioObjectPropertyAddress *inAddress,
                                     UInt32 inQualifierDataSize,
                                     const void *inQualifierData,
                                     UInt32 inDataSize, const void *inData) {
  (void)inDriver;
  (void)inObjectID;
  (void)inClientProcessID;
  (void)inAddress;
  (void)inQualifierDataSize;
  (void)inQualifierData;
  (void)inDataSize;
  (void)inData;
  return kAudioHardwareUnknownPropertyError;
}

static OSStatus Keel_StartIO(AudioServerPlugInDriverRef inDriver,
                             AudioObjectID inDeviceObjectID,
                             UInt32 inClientID) {
  (void)inDriver;
  (void)inDeviceObjectID;
  (void)inClientID;
  keel_shm_open();
  gIORunning = true;
  gNumberTimeStamps = 0;
  gSampleTime = 0.0;
  return noErr;
}

static OSStatus Keel_StopIO(AudioServerPlugInDriverRef inDriver,
                            AudioObjectID inDeviceObjectID,
                            UInt32 inClientID) {
  (void)inDriver;
  (void)inDeviceObjectID;
  (void)inClientID;
  gIORunning = false;
  return noErr;
}

static OSStatus Keel_GetZeroTimeStamp(AudioServerPlugInDriverRef inDriver,
                                      AudioObjectID inDeviceObjectID,
                                      UInt32 inClientID, Float64 *outSampleTime,
                                      UInt64 *outHostTime,
                                      UInt64 *outSeed) {
  (void)inDriver;
  (void)inDeviceObjectID;
  (void)inClientID;
  if (!outSampleTime || !outHostTime || !outSeed)
    return kAudioHardwareIllegalOperationError;
  *outSampleTime = gSampleTime;
  *outHostTime = mach_absolute_time();
  *outSeed = 1;
  return noErr;
}

static OSStatus Keel_WillDoIOOperation(AudioServerPlugInDriverRef inDriver,
                                       AudioObjectID inDeviceObjectID,
                                       UInt32 inClientID, UInt32 inOperationID,
                                       Boolean *outWillDo,
                                       Boolean *outWillDoInPlace) {
  (void)inDriver;
  (void)inDeviceObjectID;
  (void)inClientID;
  if (!outWillDo || !outWillDoInPlace)
    return kAudioHardwareIllegalOperationError;
  *outWillDo = (inOperationID == kAudioServerPlugInIOOperationReadInput ||
                inOperationID == kAudioServerPlugInIOOperationWriteMix);
  *outWillDoInPlace = true;
  return noErr;
}

static OSStatus Keel_BeginIOOperation(AudioServerPlugInDriverRef inDriver,
                                      AudioObjectID inDeviceObjectID,
                                      UInt32 inClientID, UInt32 inOperationID,
                                      UInt32 inIOBufferFrameSize,
                                      const AudioServerPlugInIOCycleInfo *inIOCycleInfo) {
  (void)inDriver;
  (void)inDeviceObjectID;
  (void)inClientID;
  (void)inOperationID;
  (void)inIOBufferFrameSize;
  (void)inIOCycleInfo;
  return noErr;
}

static OSStatus Keel_DoIOOperation(AudioServerPlugInDriverRef inDriver,
                                   AudioObjectID inDeviceObjectID,
                                   AudioObjectID inStreamObjectID,
                                   UInt32 inClientID, UInt32 inOperationID,
                                   UInt32 inIOBufferFrameSize,
                                   const AudioServerPlugInIOCycleInfo *inIOCycleInfo,
                                   void *ioMainBuffer, void *ioSecondaryBuffer) {
  (void)inDriver;
  (void)inClientID;
  (void)inIOCycleInfo;
  (void)ioSecondaryBuffer;
  keel_shm_open();
  int devIndex = (int)inDeviceObjectID - 1000;
  if (devIndex < 0 || devIndex >= kKeelDevice_Count)
    devIndex = 0;

  float *ring = keel_ring(devIndex);
  KeelDeviceSlot *slot = keel_slot(devIndex);
  if (!ring || !slot || !ioMainBuffer)
    return noErr;

  /* Non-interleaved float planes in ioMainBuffer (ASBD flag). */
  float **planes = (float **)ioMainBuffer;
  float *left = planes ? planes[0] : NULL;
  float *right = (planes && KEEL_CHANNELS > 1) ? planes[1] : left;

  if (inOperationID == kAudioServerPlugInIOOperationWriteMix) {
    /* App → HAL virtual input / mix sink: store into ring. */
    uint32_t wi = slot->write_idx;
    for (UInt32 f = 0; f < inIOBufferFrameSize; f++) {
      size_t dst = ((wi + f) % KEEL_RING_FRAMES) * KEEL_CHANNELS;
      ring[dst] = left ? left[f] : 0.f;
      ring[dst + 1] = right ? right[f] : ring[dst];
    }
    slot->write_idx = wi + inIOBufferFrameSize;
  } else if (inOperationID == kAudioServerPlugInIOOperationReadInput) {
    /* HAL → app: play through from ring (keel write_output). */
    uint32_t ri = slot->read_idx;
    uint32_t avail = slot->write_idx - ri;
    for (UInt32 f = 0; f < inIOBufferFrameSize; f++) {
      float l = 0.f, r = 0.f;
      if (f < avail) {
        size_t src = ((ri + f) % KEEL_RING_FRAMES) * KEEL_CHANNELS;
        l = ring[src];
        r = ring[src + 1];
      }
      if (left)
        left[f] = l;
      if (right)
        right[f] = r;
    }
    if (avail > 0) {
      UInt32 take = avail < inIOBufferFrameSize ? avail : inIOBufferFrameSize;
      slot->read_idx = ri + take;
    }
  }

  gSampleTime += (Float64)inIOBufferFrameSize;
  gNumberTimeStamps++;
  (void)inStreamObjectID;
  return noErr;
}

static OSStatus Keel_EndIOOperation(AudioServerPlugInDriverRef inDriver,
                                    AudioObjectID inDeviceObjectID,
                                    UInt32 inClientID, UInt32 inOperationID,
                                    UInt32 inIOBufferFrameSize,
                                    const AudioServerPlugInIOCycleInfo *inIOCycleInfo) {
  (void)inDriver;
  (void)inDeviceObjectID;
  (void)inClientID;
  (void)inOperationID;
  (void)inIOBufferFrameSize;
  (void)inIOCycleInfo;
  return noErr;
}

static AudioServerPlugInDriverInterface gInterface = {
    NULL,
    Keel_QueryInterface,
    Keel_AddRef,
    Keel_Release,
    Keel_Initialize,
    NULL, /* CreateDevice */
    NULL, /* DestroyDevice */
    NULL, /* AddDeviceClient */
    NULL, /* RemoveDeviceClient */
    NULL, /* PerformDeviceConfigurationChange */
    NULL, /* AbortDeviceConfigurationChange */
    Keel_HasProperty,
    Keel_IsPropertySettable,
    Keel_GetPropertyDataSize,
    Keel_GetPropertyData,
    Keel_SetPropertyData,
    Keel_StartIO,
    Keel_StopIO,
    Keel_GetZeroTimeStamp,
    Keel_WillDoIOOperation,
    Keel_BeginIOOperation,
    Keel_DoIOOperation,
    Keel_EndIOOperation,
};

void *KeelAudio_Create(CFAllocatorRef allocator, CFUUIDRef type_uuid) {
  (void)allocator;
  (void)type_uuid;
  keel_shm_open();
  Keel_AddRef(gDriverRef);
  return gDriverRef;
}

__attribute__((used)) static const char *const *keel_device_names(void) {
  return kDeviceNames;
}
__attribute__((used)) static const char *keel_driver_name(void) {
  return KEEL_DRIVER_NAME;
}
__attribute__((used)) static const char *keel_manufacturer(void) {
  return KEEL_MANUFACTURER;
}
