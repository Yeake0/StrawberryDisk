//! Bind the loopback channel to the actual OS process at its other endpoint.
//! A session secret is useful for correlation, but must not be the only boundary
//! if another local process can observe bootstrap arguments or race a connection.

use std::{
    io,
    net::{Ipv4Addr, TcpStream},
    ptr,
};
use windows_sys::Win32::{
    Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS},
    NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
    },
    Networking::WinSock::AF_INET,
};

pub(super) fn verify_peer_process(stream: &TcpStream, expected_pid: u32) -> io::Result<()> {
    let local = stream.local_addr()?;
    let peer = stream.peer_addr()?;
    if local.ip() != Ipv4Addr::LOCALHOST || peer.ip() != Ipv4Addr::LOCALHOST {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let mut size = 0;
    let code = unsafe {
        GetExtendedTcpTable(
            ptr::null_mut(),
            &mut size,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        )
    };
    if code != ERROR_INSUFFICIENT_BUFFER {
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    // The connection table can grow between sizing and reading. Retry a bounded
    // number of times and cap memory rather than trusting the returned byte count.
    for _ in 0..3 {
        if !(4..=4 * 1024 * 1024).contains(&size) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        // DWORD storage provides the alignment required by the native table.
        let mut storage = vec![0_u32; (size as usize).div_ceil(4)];
        let code = unsafe {
            GetExtendedTcpTable(
                storage.as_mut_ptr().cast(),
                &mut size,
                0,
                AF_INET as u32,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };
        if code == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if code != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        let count = storage[0] as usize;
        let offset = std::mem::offset_of!(MIB_TCPTABLE_OWNER_PID, table);
        if count
            > (size as usize).saturating_sub(offset) / std::mem::size_of::<MIB_TCPROW_OWNER_PID>()
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        // Read only the initialized rows, after checking both returned size and allocation.
        if size as usize > storage.len() * 4 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let rows = unsafe {
            std::slice::from_raw_parts(
                storage
                    .as_ptr()
                    .cast::<u8>()
                    .add(offset)
                    .cast::<MIB_TCPROW_OWNER_PID>(),
                count,
            )
        };
        let loopback = u32::from_ne_bytes(Ipv4Addr::LOCALHOST.octets());
        let matches = rows.iter().any(|row| {
            row.dwOwningPid == expected_pid
                && row.dwLocalAddr == loopback
                && row.dwRemoteAddr == loopback
                && u16::from_be(row.dwLocalPort as u16) == peer.port()
                && u16::from_be(row.dwRemotePort as u16) == local.port()
        });
        return if matches {
            Ok(())
        } else {
            Err(io::ErrorKind::PermissionDenied.into())
        };
    }
    Err(io::ErrorKind::WouldBlock.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn channel_accepts_its_real_peer_and_rejects_a_different_process() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        verify_peer_process(&client, std::process::id()).unwrap();
        verify_peer_process(&server, std::process::id()).unwrap();
        assert!(verify_peer_process(&server, u32::MAX).is_err());
    }
}
