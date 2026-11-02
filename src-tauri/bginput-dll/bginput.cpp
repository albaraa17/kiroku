#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <tlhelp32.h>
#include <dbghelp.h>

#pragma comment(lib, "user32.lib")
#pragma comment(lib, "kernel32.lib")

static HWND g_gameHwnd = NULL;
static WNDPROC g_origWndProc = NULL;
static volatile LONG g_inSyntheticClick = 0;
static volatile ULONGLONG g_syntheticUntilTick = 0;
static POINT g_syntheticClientPt = { 375, 375 };

typedef HWND(WINAPI* PFN_GetForegroundWindow)(void);
typedef HWND(WINAPI* PFN_GetActiveWindow)(void);
typedef HWND(WINAPI* PFN_GetFocus)(void);
typedef SHORT(WINAPI* PFN_GetAsyncKeyState)(int);
typedef SHORT(WINAPI* PFN_GetKeyState)(int);
typedef BOOL(WINAPI* PFN_GetCursorPos)(LPPOINT);

static PFN_GetForegroundWindow Real_GetForegroundWindow = GetForegroundWindow;
static PFN_GetActiveWindow     Real_GetActiveWindow     = GetActiveWindow;
static PFN_GetFocus            Real_GetFocus            = GetFocus;
static PFN_GetAsyncKeyState    Real_GetAsyncKeyState    = GetAsyncKeyState;
static PFN_GetKeyState         Real_GetKeyState         = GetKeyState;
static PFN_GetCursorPos        Real_GetCursorPos        = GetCursorPos;

static BOOL IsGameTrulyForeground() {
    HWND fg = Real_GetForegroundWindow();
    if (!fg) return FALSE;
    if (fg == g_gameHwnd) return TRUE;
    DWORD fgPid = 0;
    GetWindowThreadProcessId(fg, &fgPid);
    return fgPid == GetCurrentProcessId();
}

static BOOL IsSyntheticPulseActive() {
    if (InterlockedCompareExchange(&g_inSyntheticClick, 0, 0) > 0) {
        return TRUE;
    }
    return GetTickCount64() < g_syntheticUntilTick;
}

static HWND WINAPI Hook_GetForegroundWindow(void) {
    if (g_gameHwnd && IsWindow(g_gameHwnd)) {
        return g_gameHwnd;
    }
    return Real_GetForegroundWindow();
}

static HWND WINAPI Hook_GetActiveWindow(void) {
    if (g_gameHwnd && IsWindow(g_gameHwnd)) {
        return g_gameHwnd;
    }
    return Real_GetActiveWindow();
}

static HWND WINAPI Hook_GetFocus(void) {
    if (g_gameHwnd && IsWindow(g_gameHwnd)) {
        return g_gameHwnd;
    }
    return Real_GetFocus();
}

static SHORT WINAPI Hook_GetAsyncKeyState(int vKey) {
    if (IsGameTrulyForeground()) {
        return Real_GetAsyncKeyState(vKey);
    }
    // Game is in background: only report VK_LBUTTON / VK_RETURN during a synthetic ControlClick pulse!
    if (IsSyntheticPulseActive() && (vKey == VK_LBUTTON || vKey == VK_RETURN)) {
        return (SHORT)0x8001;
    }
    // Block physical mouse clicks & keyboard presses made in Chrome/Yomitan/Anki from leaking into the game
    return 0;
}

static SHORT WINAPI Hook_GetKeyState(int vKey) {
    if (IsGameTrulyForeground()) {
        return Real_GetKeyState(vKey);
    }
    if (IsSyntheticPulseActive() && (vKey == VK_LBUTTON || vKey == VK_RETURN)) {
        return (SHORT)0x8001;
    }
    if (vKey == VK_LBUTTON || vKey == VK_RBUTTON || vKey == VK_MBUTTON) {
        return 0;
    }
    return Real_GetKeyState(vKey);
}

static BOOL WINAPI Hook_GetCursorPos(LPPOINT lpPoint) {
    if (!lpPoint) return FALSE;
    if (IsGameTrulyForeground() || !g_gameHwnd || !IsWindow(g_gameHwnd)) {
        return Real_GetCursorPos(lpPoint);
    }
    if (IsSyntheticPulseActive()) {
        POINT pt = g_syntheticClientPt;
        ClientToScreen(g_gameHwnd, &pt);
        *lpPoint = pt;
        return TRUE;
    }
    // While idle in the background, park virtual cursor at (375, 375) client space so hovering
    // the real mouse over Chrome doesn't highlight/click in-game menu buttons underneath.
    POINT pt = g_syntheticClientPt;
    ClientToScreen(g_gameHwnd, &pt);
    *lpPoint = pt;
    return TRUE;
}

static LRESULT CALLBACK SubclassWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    switch (msg) {
        case WM_ACTIVATE:
            if (LOWORD(wParam) == WA_INACTIVE) {
                // Swallow deactivation so the engine stays in active state while in the background
                return 0;
            }
            break;
        case WM_ACTIVATEAPP:
            if (wParam == FALSE) {
                return 0;
            }
            break;
        case WM_KILLFOCUS:
            return 0;
        case WM_LBUTTONDOWN:
        case WM_LBUTTONUP:
        case WM_KEYDOWN:
        case WM_KEYUP:
        case WM_CHAR:
        case WM_MOUSEWHEEL:
            if (!IsGameTrulyForeground()) {
                if (msg == WM_LBUTTONDOWN || msg == WM_LBUTTONUP) {
                    int x = (int)(short)LOWORD(lParam);
                    int y = (int)(short)HIWORD(lParam);
                    if (x > 0 && y > 0) {
                        g_syntheticClientPt.x = x;
                        g_syntheticClientPt.y = y;
                    }
                    // Also send WM_MOUSEMOVE at the target coordinate first so engines that track hover position register it
                    CallWindowProcW(g_origWndProc, hwnd, WM_MOUSEMOVE, 0, lParam);
                }
                InterlockedIncrement(&g_inSyntheticClick);
                g_syntheticUntilTick = GetTickCount64() + 35;
                LRESULT res = CallWindowProcW(g_origWndProc, hwnd, msg, wParam, lParam);
                InterlockedDecrement(&g_inSyntheticClick);
                return res;
            }
            break;
    }
    return CallWindowProcW(g_origWndProc, hwnd, msg, wParam, lParam);
}

static void PatchModuleIAT(HMODULE hMod) {
    if (!hMod) return;
    BYTE* base = (BYTE*)hMod;
    IMAGE_DOS_HEADER* dos = (IMAGE_DOS_HEADER*)base;
    if (dos->e_magic != IMAGE_DOS_SIGNATURE) return;
    IMAGE_NT_HEADERS* nt = (IMAGE_NT_HEADERS*)(base + dos->e_lfanew);
    if (nt->Signature != IMAGE_NT_SIGNATURE) return;

    IMAGE_DATA_DIRECTORY importDir = nt->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_IMPORT];
    if (!importDir.VirtualAddress || !importDir.Size) return;

    IMAGE_IMPORT_DESCRIPTOR* desc = (IMAGE_IMPORT_DESCRIPTOR*)(base + importDir.VirtualAddress);
    for (; desc->Name; ++desc) {
        const char* modName = (const char*)(base + desc->Name);
        if (lstrcmpiA(modName, "user32.dll") != 0) continue;

        IMAGE_THUNK_DATA* origThunk = desc->OriginalFirstThunk
            ? (IMAGE_THUNK_DATA*)(base + desc->OriginalFirstThunk)
            : NULL;
        IMAGE_THUNK_DATA* iatThunk = (IMAGE_THUNK_DATA*)(base + desc->FirstThunk);
        if (!iatThunk) continue;

        for (; iatThunk->u1.Function; ++iatThunk, origThunk ? ++origThunk : NULL) {
            void* funcPtr = (void*)iatThunk->u1.Function;
            void* replacement = NULL;

            if (funcPtr == (void*)Real_GetForegroundWindow) {
                replacement = (void*)Hook_GetForegroundWindow;
            } else if (funcPtr == (void*)Real_GetActiveWindow) {
                replacement = (void*)Hook_GetActiveWindow;
            } else if (funcPtr == (void*)Real_GetFocus) {
                replacement = (void*)Hook_GetFocus;
            } else if (funcPtr == (void*)Real_GetAsyncKeyState) {
                replacement = (void*)Hook_GetAsyncKeyState;
            } else if (funcPtr == (void*)Real_GetKeyState) {
                replacement = (void*)Hook_GetKeyState;
            } else if (funcPtr == (void*)Real_GetCursorPos) {
                replacement = (void*)Hook_GetCursorPos;
            }

            if (replacement) {
                DWORD oldProt = 0;
                if (VirtualProtect(&iatThunk->u1.Function, sizeof(void*), PAGE_EXECUTE_READWRITE, &oldProt)) {
                    iatThunk->u1.Function = (ULONG_PTR)replacement;
                    VirtualProtect(&iatThunk->u1.Function, sizeof(void*), oldProt, &oldProt);
                }
            }
        }
    }
}

static void PatchAllModulesIAT() {
    DWORD pid = GetCurrentProcessId();
    HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid);
    if (snap == INVALID_HANDLE_VALUE) {
        PatchModuleIAT(GetModuleHandleW(NULL));
        return;
    }
    HMODULE selfMod = NULL;
    GetModuleHandleExW(
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        (LPCWSTR)&PatchAllModulesIAT,
        &selfMod
    );

    MODULEENTRY32W me;
    me.dwSize = sizeof(me);
    if (Module32FirstW(snap, &me)) {
        do {
            if (me.hModule != selfMod) {
                PatchModuleIAT(me.hModule);
            }
        } while (Module32NextW(snap, &me));
    }
    CloseHandle(snap);
}

static BOOL CALLBACK FindGameWindowProc(HWND hwnd, LPARAM lParam) {
    DWORD pid = 0;
    GetWindowThreadProcessId(hwnd, &pid);
    if (pid != GetCurrentProcessId()) return TRUE;
    if (!IsWindowVisible(hwnd)) return TRUE;
    LONG exStyle = GetWindowLongW(hwnd, GWL_EXSTYLE);
    if (exStyle & WS_EX_TOOLWINDOW) return TRUE;
    if (GetWindowTextLengthW(hwnd) <= 0) return TRUE;

    g_gameHwnd = hwnd;
    return FALSE;
}

static DWORD WINAPI InitBgInputThread(LPVOID lpParam) {
    // Wait up to 10 seconds for the game's main visible window to exist
    for (int attempt = 0; attempt < 40; ++attempt) {
        EnumWindows(FindGameWindowProc, 0);
        if (g_gameHwnd && IsWindow(g_gameHwnd)) {
            break;
        }
        Sleep(250);
    }

    PatchAllModulesIAT();

    if (g_gameHwnd && IsWindow(g_gameHwnd) && !g_origWndProc) {
        g_origWndProc = (WNDPROC)SetWindowLongPtrW(g_gameHwnd, GWLP_WNDPROC, (LONG_PTR)SubclassWndProc);
        // Immediately tell the game window it is active & focused so background ControlClick works right away
        PostMessageW(g_gameHwnd, WM_ACTIVATEAPP, TRUE, 0);
        PostMessageW(g_gameHwnd, WM_ACTIVATE, WA_ACTIVE, 0);
        PostMessageW(g_gameHwnd, WM_SETFOCUS, 0, 0);
    }
    return 0;
}

BOOL APIENTRY DllMain(HMODULE hModule, DWORD ul_reason_for_call, LPVOID lpReserved) {
    if (ul_reason_for_call == DLL_PROCESS_ATTACH) {
        DisableThreadLibraryCalls(hModule);
        HANDLE hThread = CreateThread(NULL, 0, InitBgInputThread, NULL, 0, NULL);
        if (hThread) CloseHandle(hThread);
    }
    return TRUE;
}
