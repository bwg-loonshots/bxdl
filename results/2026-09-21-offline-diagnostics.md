# 오프라인 logs/diagnose 구현 검증

- 날짜: 2026-09-21
- 기준: BXDL `8bb6ea4` 이후 `feat/instance-diagnostics` 변경. 로컬 검증 기록이며 원격 CI·공식 릴리스 기록은 아니다.
- 공급 계약: 기존 NIGO clean `303e163a`/PROPOSED 유지. NIGO 공유 checkout·원장 변경 없음.
- 연결: [설계](../design/2026-09-21-offline-diagnostics.md), [명령](../docs/cli.md), [구현 상태](../docs/implementation-status.md).

## 변경 범위

`logs --instance`와 `diagnose --instance --output`을 구현했다. 현재 초기화·서비스 시도의 private journal/JSONL을 제한 범위에서 읽고 고정 필드만 반환한다. 식별자·경로·hash·설정·credential·자유 텍스트를 출력하지 않으며 stdout/stderr는 본문을 읽지 않는다. 사람용 출력은 저장 상태·사건·정책 제외·자료 누락·다음 조치를 구분하고 JSON mode는 기존 envelope를 유지한다.

support JSON은 이미 존재하는 부모 아래 새0600 파일로 게시한다. control/data/package와 등록 입력의 겹침, symlink·hardlink·FIFO·부적절한 권한, 기존 출력 덮어쓰기를 거부한다. 손상된 binding과 journal의 결속이 없으면 임의의 보호 경로를 신뢰하지 않고 파일 export를 거부한다. 읽을 수 있는 logs는 partial로 남긴다.

기존 startup gate의 판정 로직을 별도 decode 함수로 추출해 같은 service journal 검사를 진단에서도 재사용했다. Store에는 record 크기를 읽기 전에 제한하고 이미 읽은 snapshot을 사용하는 API를 추가했다. 기존 open/read의 기본 한도·변경 감지·쓰기 규칙은 유지한다.

## 자동 검사

| 검사 | 결과 |
| --- | --- |
| `make check` | PASS: fmt·all-targets Clippy `-D warnings`·232 tests |
| `cargo build --locked --release` | PASS: macOS arm64 CLI |
| `make check-linux` | PASS: x86_64-unknown-linux-gnu 타입 검사. Linux 실행·systemd 인수는 아님 |

232개는 library 207, CLI 13, CLI integration 2, setup CLI 5, setup safety 5다. 기존 200개 대비 32개 추가: 이벤트 projection 7, private I/O 10, collector 11, CLI parser 3, 사람용 출력 1.

검사에는 canary가 든 details/ID/reason/원문 로그, 잘린 JSONL·빈 report·알 수 없는 code, sequence/PID/attempt 불일치, payload 예산과 만료, 정상 정지·UNKNOWN·잠금 유지, binding–journal 불일치로 출력 범위를 바꾸려는 사례, 기존 파일/심볼릭·하드 링크/FIFO·Mac case alias, 동시 export 한 번만 게시, 원본 자료 불변을 포함한다. 알 수 없는 내용을 오류 문자열로 다시 노출하지 않는다.

## 실제 보존 기록 검증

9월 18일 실제 Mac package의 LaunchAgent 시험 후 정상 종료한 인스턴스 두 곳을 대상으로 **이번 release CLI**를 실행했다. 하나는 `/private/tmp`, 다른 하나는 기본 `Library/Application Support/BXDL`의 공백 경로다. 이전 설치본·worker를 덮어쓰거나 새 엔진을 시작하지 않았다. 각 기록의 NIGO source는 같은 `303e163a`다.

두 곳 모두 다음을 확인했다.

- diagnose exit0, 초기화 3개·서비스 5개인 총8개 고정 이벤트, 저장 상태 INITIALIZED/STOPPED_VERIFIED.
- runtimeReadiness는 NOT_OBSERVED를 유지. 사람이 읽는 출력도 현재 상태를 관측하지 않았음을 표시.
- 진단 JSON과 CLI data 일치, 파일 권한0600, instance ID·원본 경로·archive hash 미포함.
- `logs --tail 2`는 이벤트2개와 생략 표시·partial/exit5.
- 기존 output 재사용과 control 내부 output 시도 거부, 기존 파일 내용 불변.
- 수집 전후 control의 기록/JSONL/plist/log와 data 파일 내용 hash 동일. atime 등 filesystem 메타데이터 불변을 주장하지 않음.

첫 sandbox 실행에서는 advisory 잠금 관측을 사용할 수 없어 source 누락·partial/exit5가 기록됐다. 이를 성공으로 바꾸지 않았으며 같은 로컬 시험에 필요한 권한이 허용된 실행에서 두 곳 모두 완료를 확인했다. 실제 노드나 시험 job을 추가로 실행하지 않았다.

| 최종 산출물 | SHA-256 |
| --- | --- |
| 이번 release CLI | `764c8dba2a8340ba6b8542d3f56a42c4d28fac4ab9714c1d8b631d2536a5e5d5` |
| 임시 경로의 정제 report | `08bbe8b615c235fb0dcb75100a21c0182200e7cde07cfb0809ed59df3c76b575` |
| Application Support의 정제 report | `776e31ab6f525fe85c20afb1da2674444513a048eaf709f364a7d4c19a9337d9` |

`Cargo.toml`, `Cargo.lock`, `src/**/*.rs`의 repository-relative 경로를 Python `sorted(Path 목록)`으로 정렬하고 POSIX 경로의 `path + NUL + bytes + NUL`을 hash한 구현 digest는 `1ccf7cab253f2a2d242702906a1b7539b6aaa4264d45b3bd0cd6f31dce01647c`다. 이 값은 로컬 재현 보조 자료이며 서명된 provenance가 아니다.

## 남은 범위

- 이번 CLI를 새 배포 archive로 조립·설치한 전체 패키지 인수와 원격 CI는 별도다. 과거 package의 진단 기록 소비를 새 package 전체 승인으로 확대하지 않는다.
- 실제 실행 중 계속 증가하는 로그와 파일시스템 장애는 부정 fixture로 변경 감지했으며, 장시간 부하·power-cut·멈춘 filesystem syscall의 hard deadline을 검증하지 않았다.
- 전체 과거 시도·raw/follow·로그 rotation·보존/용량 자동 관리, UNKNOWN 조정·안전한 제거는 후속이다.
- setup 전체 연결, 사용자 세션 장애·4-validator·오프라인 G1-M, 정식 JRE·서명 공급, Linux/systemd·Docker는 별도 인수다.
