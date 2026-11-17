# LNX-03 Linux 개발용 systemd 구현·자동 검사

- 기준: LNX-02 commit `fe84ca6abde288e911dfd91cfef7753979fbcdce` 이후 working tree. 이 기록 시점에 LNX-03은 미커밋이다.
- 상태: **DEVELOPMENT_BACKEND_AUTO_CHECKED**. 실제 Linux JVM·systemd·계정 격리·RocksDB 실행은 **NOT_RUN**이다.
- [정제 JSON](./2026-11-16-linux-systemd-development.json), [관리자 개발 안내](../docs/linux-service-development.md), [단계 계획](../design/2026-11-16-linux-first-implementation-plan.md).

## 구현

별도 Linux control 형식으로 root binding/journal·고정 공급물·등록 worker와 서비스 UID의 data/output을 분리했다. root가 승인한 spec과 선정 Linux runtime/engine lock을 대조하고, 로컬 전용 nologin UID/GID를 요구한다. 계정 생성이나 sudoers 변경을 자동 수행하지 않는다. 등록 단계의 공급 보관과 cold 검증은 명시 init/start와 별도다.

기존 `init/resume-init/start/status/stop --instance`를 Linux 경로에 연결했다. 모든 JVM 명령은 고정 setpriv를 통해 비root UID/GID로 실행한다. root 제어 잠금의 읽기 전용 descriptor만 상속한다. 직접 systemctl 호출도 같은 일회성 root gate를 거치며 unit·InvocationID·PID/start ticks·cgroup·실제 credentials·엔진 보고서를 결속한다.

정상 종료는 root ExecStopPost 기록의 동일 시도·정상 exit, 엔진 STOPPED/storage·consensus closure, PID/cgroup 부재와 Java/RocksDB 잠금 해제 조건을 함께 요구한다. systemd GC로 과거 메모리 정보가 사라져도 root 종료 기록을 검증한다. 자동 restart/enable은 없으며 미해결 기록은 UNKNOWN으로 보존한다.

`logs/diagnose`는 저장 제어 상태·검증 가능한 최종 report 요약만 수집한다. 전체 사건·system journal·raw 출력은 제외하므로 항상 partial/INCOMPLETE로 표시한다. 일반 Linux 설치·이름 연결·고정 실행기 위임 profile·전체 bounded event collector는 후속 범위다. Mac 기존 등록·LaunchAgent와 RC.4 공급 pin은 바꾸지 않았다.

## 검사 근거

| 검사 | 결과·범위 |
| --- | --- |
| `RUST_TEST_THREADS=4 make check` | fmt·Clippy PASS, Rust **659 PASS**, Python **205 PASS·기존 opt-in 2 SKIP** |
| `make check-linux` | `x86_64-unknown-linux-gnu --all-targets` 타입 검사 PASS. Linux link/run 아님 |
| Mac CLI smoke | 계정 이름 계산1건·Linux 등록/gate/exit의 foreign-host 거절3건 PASS. 서비스 변경0회 |
| 독립 검토 | descriptor 기준 입력·DB 파일 검사, 그룹/권한 제거 관측, init 재개 조건, gate/종료 잠금 경합, systemd GC 종료 기록, stop 시도 재확인 보강 |

기본 병렬 Rust 회귀 실행에서는 기존 Mac executor의 시간 민감 검사2개가 실패했다(라이브러리488 PASS·2 FAIL). 변경하지 않은 executor 모듈21개를 순차 재실행해 모두 통과했고, 이후 같은 최종 코드의 전체 검사를 동시4개로 실행해 통과했다. 병렬 부하의 영향이 의심되지만 원인을 단정하지 않는다. 테스트 제한 시간·assertion을 완화하거나 실패를 누락하지 않았다. 로그 digest와 검증한 코드 digest는 JSON에 기록했다.

실제 Linux가 없으므로 신규 OS 서비스·전용 계정 변경은 수행하지 않았다. root가 아닌 실제 Java credentials, 직접 systemctl 재시작 거부, 종료/중단/SSH 연결 해제, 실제 Linux JRE/native/RocksDB 조합은 테스터의 별도 실행으로 확인해야 한다. 이 결과는 LNX-03의 실물 완료 기준 또는 고객 배포 승인이 아니다.

## 다음 연결

1. 실제 Linux build/cold/native 결과 수신과 R-L/A-L/C-L 공급물 조립·고정.
2. root 설치·전용 계정·경로 검토 UX, 이름 연결, 고정 실행기 위임과 전체 진단 수집 통합.
3. 같은 조합의 설치→init→서비스→같은 DB 재시작·SSH 단절·CLI 전환을 실물 인수.
4. Linux 사용자 과업·서버/네트워크 인수. WSL2는 후속 유지.

SSH 정보를 공유하지 않아도 테스터가 직접 실행하고 정제 결과를 전달할 수 있다. 이번 작업에서 NIGO 저장소는 변경하지 않았다.
