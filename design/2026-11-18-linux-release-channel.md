# NIGO 포함 Linux 패키지의 다운로드 배포 설계와 구현 계획

## 설치 UX와 배포 단위

Ubuntu 24.04 x86_64 일반 사용자는 BXDL GitHub Releases에서 태그를 선택하고 설치 명령을 복사한다. 설치 스크립트가 미리 빌드한 CLI와 NIGO JAR·전용 실행기·JRE가 들어 있는 runtime archive를 내려받는다. 한 번의 설치 요청으로 두 배포물을 준비하며, 관리 CLI 버전과 등록된 노드 실행기의 버전은 독립적으로 유지한다. 사용자가 native 설정과 자격 자료를 선택하고 `setup apply`, `init`, `start`를 각각 승인한다. 명령 예시와 실제 경로는 [다운로드 설치](../docs/linux-release.md)에서 관리한다.

버전별 URL은 `https://github.com/bwg-loonshots/bxdl/releases/download/<tag>/<asset>`이다. 처음 게시할 채널은 Pre-release다. 배포 파일은 다음과 같다.

| 파일 | 역할 |
| --- | --- |
| `install-bxdl.sh`, `install-linux-helper.sh` | 일반 사용자 설치와 선택한 관리자 실행기 배치 |
| `bxdl-linux-amd64` | 서명된 목록과 대조해 처음 실행하는 Linux 검증 CLI |
| `bxdl-cli-linux-amd64.tar.gz` | 서명된 독립 관리 CLI·자동완성·운용 안내 |
| `bxdl-runtime-linux-amd64.tar.gz` | 서명된 고정 실행기·RC.4 JAR·Temurin JRE 전체·제품 고지와 SBOM |
| `release.json`, `release-public.pem` | 태그·소스·구성 식별 정보와 서명 공개키 사본 |
| `SHA256SUMS`, `SHA256SUMS.sig` | 태그와 위 일곱 파일 이름·해시가 포함된 목록, 그 원문에 대한 Ed25519 서명 |

신뢰의 출발점은 배포 담당자가 별도 경로로 제공한 공개키와 버전별 부트스트랩 스크립트다. 최초 스크립트도 실행 전에 검증한다. 공식 릴리스 페이지를 신뢰하는 경로에서는 페이지의 스크립트 SHA-256을 대조한다. 오프라인 배포 담당자는 외부 공개키로 SHA256SUMS 서명을 먼저 검사하고 해당 스크립트 행의 해시를 대조할 수 있다. 묶음의 공개키 사본은 키 배포 수단을 대체하지 않는다. 시스템 helper 교체와 관리 CLI 업데이트의 책임을 구분하며 동일 태그 자산은 보존한다.

## 단계별 구현과 인수 기준

| 단계 | 산출물 | 완료 기준 |
| --- | --- | --- |
| 1. 제작 | [릴리스 제작 도구](../tools/build-linux-release.py) | clean commit의 Linux 실행기·공급 lock·실제 JAR/JRE·고지 자료로 두 archive 제작·검증, 태그와 전체 파일 해시 서명 |
| 2. 설치 | [사용자 스크립트](../scripts/install-bxdl.sh), [helper 설치](../scripts/install-linux-helper.sh), Rust `install` | 다운로드 또는 로컬 파일로 같은 검증 수행, Linux 서명 archive 추출·재실행 검증, 관리 CLI 설치와 기존 setup 연결 |
| 3. 배포 | [GitHub Actions](../.github/workflows/linux-release.yml) | 지정 태그에서 draft 제작. 공개 시 인수한 SHA256SUMS와 원격 파일을 재검사하고 같은 draft를 Pre-release로 전환 |
| 4. 최초 공개 | 배포 조건을 승인한 NIGO JAR·제품 NOTICE/SBOM·대응 소스·릴리스 키·실제 장비 결과 | 인수자가 draft 자산으로 설치·초기화·systemd 운용 확인 후 유지관리자가 publish 실행 |
| 5. 설치 경로 확인 | 공개 URL과 새 Ubuntu 사용자 환경 | 릴리스 페이지의 curl 명령으로 공개 자산 다운로드와 동일 설치 결과를 관측해 기록 |

1~3 단계는 이 변경의 구현 범위다. 4~5 단계의 실제 수행 기록과 공개 URL은 첫 릴리스에 기록한다. 기존 비공개 빌드 키트는 그 전달본에 고정한 절차로 시험한다.

## 배포 담당자 실행 절차

1. NIGO 담당자에게 공개 허용 **JAR SHA-256과 조건**을 확인한다. 승인한 NIGO JAR·native/Console, Temurin과 Rust CLI를 포괄하는 NOTICE/SBOM과 필요한 대응 소스 제공 방법을 준비한다. RC.4 이후 JAR가 바뀌면 새 공급 lock과 조합 인수를 먼저 반영한다.
2. 공개할 태그(예: `v0.1.0-preview.1`)의 소스에 `packaging/public-releases/<tag>/`를 포함한다. `release-public.pem`은 배포 키에서 내보낸 Ed25519 SPKI PEM, `THIRD_PARTY_NOTICES`와 `SBOM.json`은 해당 제품 전체의 자료다. `RELEASE.md`에는 적용 조건·권리 근거·대응 소스 제공 경로·호환 OS·시험 범위와 제한을 적는다. 공개 저장소에 넣을 수 있는 자료만 사용한다.
3. GitHub Environment `bxdl-linux-release`에 승인된 JAR digest를 변수 `BXDL_PUBLIC_ENGINE_SHA256`으로 등록한다. secret은 RC.4 공급 저장소에만 읽기 권한이 있는 `NIGO_RELEASE_READ_TOKEN`과 Ed25519 PKCS8 PEM `BXDL_RELEASE_SIGNING_KEY_PEM`이다. 태그 보호·배포 가능 태그·검토 담당자를 설정하고 사용자에게 공개키를 별도 경로로 배포한다.
4. 워크플로가 기본 브랜치에 반영되고 검토된 태그가 원격에 존재하면 `gh workflow run linux-release.yml --ref "$TAG" -f operation=draft`를 실행한다. Ubuntu 빌더에서 전체 검사·native CLI 빌드, 해시를 고정한 NIGO private release와 Temurin 수신, 서명·설치 검사를 수행하고 새 draft를 만든다. 작성자와 인수자는 GitHub의 draft 접근 권한을 사용한다.
5. 인수자는 **그 draft**의 자산으로 새 Ubuntu 24.04 서버에서 로컬 파일 설치, helper 배치, 노드 설정·초기화, READY·정상 종료·같은 DB 재시작을 확인하고 `sha256sum SHA256SUMS` 결과를 전달한다. 실패한 draft는 공개하지 않고 수정한 후보에 새 태그를 사용한다.
6. 담당자는 `gh workflow run linux-release.yml --ref "$TAG" -f operation=publish -f accepted_sums_sha256="$ACCEPTED_SHA256"`로 게시한다. 워크플로는 draft를 다시 받아 승인된 해시, 서명·파일 구성, 소스 commit, 설치, 원격 자산 digest를 검사하고 원래 파일을 공개한다. 릴리스 설명에 해당 태그와 스크립트 해시가 들어 있는 curl 명령이 생긴다. 마지막 단계에서 인증 정보 없이 공개 URL로 스크립트를 다시 받고 `--verify-only`로 공개된 모든 자산을 대조한다. 이 다운로드 확인만 실패하면 배포 상태를 먼저 확인하고 같은 공개 URL에서 검사를 다시 수행한다.

준비와 권한 경계는 [NIGO RC.4 수신 기록](../contracts/nigo/engine-rc4-2026-11-15/README.md), [지원 범위](../docs/support-matrix.md), [Linux 인수 계획](./2026-11-16-linux-first-implementation-plan.md)을 따른다. 키 교체·기존 설치 업데이트 시에는 새 공개키를 별도로 배포하고, 기존 노드의 실행기·JAR·DB 변경은 호환성 인수를 거쳐 별도 릴리스로 제공한다.
