// Windows 11 task buttons are XAML elements, not the compatibility HWND strip.
// A brief diagnostics enumeration obtains weak references on Explorer's UI
// thread. A private message window then owns the reversible property lease.
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <ocidl.h>
#include <xamlom.h>
#include <windows.ui.xaml.hosting.desktopwindowxamlsource.h>
#undef GetCurrentTime
#include <winrt/base.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.UI.Xaml.h>
#include <winrt/Windows.UI.Xaml.Hosting.h>
#include <winrt/Windows.UI.Xaml.Media.h>
#include <memory>
#include <cmath>
#include <string>
#include <vector>
namespace ux = winrt::Windows::UI::Xaml;
static HMODULE bridge_module;
#include "xaml_lease.hpp"

namespace {
constexpr GUID tap_id = {0x64228a98, 0x4149, 0x4b44, {0xb2,0x91,0x2b,0xdd,0xd2,0xf6,0x73,0x11}};
thread_local std::vector<winrt::weak_ref<ux::FrameworkElement>> frames;
thread_local std::vector<winrt::weak_ref<ux::Hosting::DesktopWindowXamlSource>> sources;

HWND find_channel() {
    DWORD shell_pid = 0;
    GetWindowThreadProcessId(FindWindowW(L"Shell_TrayWnd", nullptr), &shell_pid);
    HWND found = nullptr;
    while ((found = FindWindowExW(nullptr, found, taskbar::channel_class().c_str(), nullptr))) {
        DWORD pid = 0;
        GetWindowThreadProcessId(found, &pid);
        if (pid && pid == shell_pid) return found;
    }
    return nullptr;
}

void reconcile() {
    HWND shell = FindWindowW(L"Shell_TrayWnd", nullptr);
    if (!shell || find_channel()) return;
    for (auto const& weak_source : sources) {
        auto source = weak_source.get();
        if (!source || !source.Content()) continue;
        auto native = source.try_as<IDesktopWindowXamlSourceNative>();
        HWND host = nullptr;
        if (!native || FAILED(native->get_WindowHandle(&host)) ||
            !host || GetAncestor(host, GA_ROOT) != shell) continue;
        for (auto const& weak_frame : frames) {
            auto frame = weak_frame.get();
            if (!frame || frame.XamlRoot() != source.Content().XamlRoot()) continue;
            WNDCLASSW definition{};
            definition.lpfnWndProc = taskbar::window_proc;
            definition.hInstance = bridge_module;
            definition.lpszClassName = taskbar::channel_class().c_str();
            RegisterClassW(&definition);
            auto lease = std::make_unique<taskbar::Lease>(frame, host);
            CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, taskbar::channel_class().c_str(), L"", WS_POPUP, 0, 0, 0, 0,
                                         nullptr, nullptr, bridge_module, &lease);
            return;
        }
    }
}

struct Watcher : winrt::implements<Watcher, IVisualTreeServiceCallback2, winrt::non_agile> {
    winrt::com_ptr<IXamlDiagnostics> diagnostics;
    explicit Watcher(IUnknown* site) { winrt::check_hresult(site->QueryInterface(diagnostics.put())); }
    HRESULT STDMETHODCALLTYPE OnVisualTreeChange(ParentChildRelation, VisualElement element,
                                                 VisualMutationType mutation) noexcept override {
        try {
            if (mutation != Add || !element.Type) return S_OK;
            bool frame_type = wcscmp(element.Type, L"Taskbar.TaskbarFrame") == 0;
            bool source_type = wcscmp(element.Type, L"Windows.UI.Xaml.Hosting.DesktopWindowXamlSource") == 0;
            if (!frame_type && !source_type) return S_OK;
            winrt::Windows::Foundation::IInspectable object{nullptr};
            winrt::check_hresult(diagnostics->GetIInspectableFromHandle(element.Handle,
                reinterpret_cast<::IInspectable**>(winrt::put_abi(object))));
            if (frame_type) frames.push_back(winrt::make_weak(object.as<ux::FrameworkElement>()));
            if (source_type) sources.push_back(winrt::make_weak(object.as<ux::Hosting::DesktopWindowXamlSource>()));
            // Match the island's real HWND to the primary Shell_TrayWnd. Equal
            // monitor widths or UI thread IDs cannot identify a primary taskbar.
            reconcile();
        } catch (...) { return winrt::to_hresult(); }
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE OnElementStateChanged(InstanceHandle, VisualElementState,
                                                     LPCWSTR) noexcept override { return S_OK; }
};

struct Tap : winrt::implements<Tap, IObjectWithSite, winrt::non_agile> {
    HRESULT STDMETHODCALLTYPE SetSite(IUnknown* site) noexcept override {
        try {
            if (!site) return S_OK;
            auto watcher = winrt::make_self<Watcher>(site);
            auto raw = watcher.detach();
            HANDLE thread = CreateThread(nullptr, 0, [](void* context) -> DWORD {
                winrt::com_ptr<Watcher> instance;
                instance.attach(static_cast<Watcher*>(context));
                try {
                    auto service = instance->diagnostics.as<IVisualTreeService3>();
                    // Enumeration can synchronously dispatch to Explorer's UI
                    // thread. Never wait for that dispatch from the UI thread.
                    HRESULT result = service->AdviseVisualTreeChange(instance.get());
                    if (SUCCEEDED(result)) service->UnadviseVisualTreeChange(instance.get());
                } catch (...) {}
                return 0;
            }, raw, 0, nullptr);
            if (!thread) { raw->Release(); return HRESULT_FROM_WIN32(GetLastError()); }
            CloseHandle(thread);
            return S_OK;
        } catch (...) { return winrt::to_hresult(); }
    }
    // The temporary enumeration owns the site only until it disconnects. The
    // idle bridge must not keep a diagnostics session or strong XAML tree alive.
    HRESULT STDMETHODCALLTYPE GetSite(REFIID, void** result) noexcept override {
        *result = nullptr;
        return E_NOINTERFACE;
    }
};

struct Factory : winrt::implements<Factory, IClassFactory, winrt::non_agile> {
    HRESULT STDMETHODCALLTYPE CreateInstance(IUnknown* outer, REFIID iid, void** result) noexcept override {
        try {
            if (outer) return CLASS_E_NOAGGREGATION;
            *result = nullptr;
            return winrt::make<Tap>().as(iid, result);
        } catch (...) { return winrt::to_hresult(); }
    }
    HRESULT STDMETHODCALLTYPE LockServer(BOOL) noexcept override { return S_OK; }
};

HRESULT attach() {
    DWORD pid = 0;
    GetWindowThreadProcessId(FindWindowW(L"Shell_TrayWnd", nullptr), &pid);
    if (!pid) return E_PENDING;
    HMODULE xaml = LoadLibraryW(L"Windows.UI.Xaml.dll");
    if (!xaml) return HRESULT_FROM_WIN32(GetLastError());
    auto initialize = reinterpret_cast<decltype(&InitializeXamlDiagnosticsEx)>(
        GetProcAddress(xaml, "InitializeXamlDiagnosticsEx"));
    HRESULT result = E_NOINTERFACE;
    wchar_t path[32768];
    DWORD length = GetModuleFileNameW(bridge_module, path, 32768);
    if (initialize && length && length < 32768) {
        for (int index = 1; index <= 10; ++index) {
            auto endpoint = L"VisualDiagConnection" + std::to_wstring(index);
            result = initialize(endpoint.c_str(), pid, L"", path, tap_id, L"");
            if (SUCCEEDED(result) || result != HRESULT_FROM_WIN32(ERROR_NOT_FOUND)) break;
        }
    }
    FreeLibrary(xaml);
    return result;
}
} // namespace

STDAPI DllGetClassObject(REFCLSID clsid, REFIID iid, void** result) {
    try {
        if (clsid != tap_id) return CLASS_E_CLASSNOTAVAILABLE;
        return winrt::make<Factory>().as(iid, result);
    } catch (...) { return winrt::to_hresult(); }
}
// Explorer's dormant message procedure still resides in this module. The cache
// file can remain mapped until Explorer exits; installation files are unaffected.
STDAPI DllCanUnloadNow() { return S_FALSE; }

extern "C" HRESULT WINAPI StrawberryTaskbarApply(UINT width, UINT height, UINT gap, UINT placement, wchar_t const* log_file, RECT* bounds) {
    try {
        static ULONGLONG next_attach = 0;
        static HRESULT last_attach = E_PENDING;
        static DWORD attached_pid = 0;
        static ULONGLONG discovery_started = 0;
        HWND channel = find_channel();
        if (!channel) {
            DWORD shell_pid = 0;
            GetWindowThreadProcessId(FindWindowW(L"Shell_TrayWnd", nullptr), &shell_pid);
            ULONGLONG now = GetTickCount64();
            if (shell_pid != attached_pid) discovery_started = now;
            // A diagnostics connection is not proof that this shell exposes the
            // expected taskbar island. Unsupported/custom shells must report a
            // capability failure instead of hiding the monitor forever as busy.
            if (SUCCEEDED(last_attach) && discovery_started && now - discovery_started > 5000) {
                last_attach = HRESULT_FROM_WIN32(ERROR_NOT_SUPPORTED);
                next_attach = now + 30000;
                taskbar::receipt(log_file, L"resident_taskbar_xaml_unavailable reason=primary_island_not_found", true);
                return last_attach;
            }
            if (shell_pid == attached_pid && GetTickCount64() < next_attach) {
                return FAILED(last_attach) ? last_attach : E_PENDING;
            }
            HRESULT result = attach();
            if (FAILED(last_attach)) discovery_started = now;
            next_attach = now + (FAILED(result) ? 30000 : 2000);
            if (result != last_attach || shell_pid != attached_pid) {
                taskbar::receipt(log_file, L"resident_taskbar_xaml_attach code=" +
                                 std::to_wstring(result) + L" shell_pid=" + std::to_wstring(shell_pid), FAILED(result));
            }
            last_attach = result;
            attached_pid = shell_pid;
            return FAILED(result) ? result : E_PENDING;
        }
        taskbar::Request request;
        request.owner = GetCurrentProcessId();
        request.width = width;
        request.height = height;
        request.gap = gap;
        request.placement = static_cast<taskbar::Placement>(placement);
        if (wcsncpy_s(request.log_file, log_file, _TRUNCATE) != 0) return E_INVALIDARG;
        COPYDATASTRUCT packet{taskbar::protocol, sizeof(request), &request};
        DWORD_PTR reply = E_FAIL;
        if (!SendMessageTimeoutW(channel, WM_COPYDATA, 0, reinterpret_cast<LPARAM>(&packet),
                                 SMTO_ABORTIFHUNG | SMTO_BLOCK, 1000, &reply)) {
            DWORD code = GetLastError();
            return HRESULT_FROM_WIN32(code ? code : ERROR_TIMEOUT);
        }
        HRESULT result = static_cast<HRESULT>(reply);
        if (SUCCEEDED(result) && (!bounds || !GetWindowRect(channel, bounds))) return E_PENDING;
        return result;
    } catch (...) { return winrt::to_hresult(); }
}

extern "C" void WINAPI StrawberryTaskbarRelease() {
    if (HWND channel = find_channel()) {
        DWORD_PTR result = 0;
        SendMessageTimeoutW(channel, taskbar::release_message, GetCurrentProcessId(), 0,
                            SMTO_ABORTIFHUNG | SMTO_BLOCK, 500, &result);
    }
}

BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, void*) {
    if (reason == DLL_PROCESS_ATTACH) bridge_module = instance;
    return TRUE;
}
