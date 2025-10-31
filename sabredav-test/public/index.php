<?php

require_once __DIR__ . '/../vendor/autoload.php';

// MySQL database configuration
$pdo = new PDO(
    'mysql:host=' . getenv('MYSQL_HOST') . ';dbname=' . getenv('MYSQL_DATABASE'),
    getenv('MYSQL_USER'),
    getenv('MYSQL_PASSWORD')
);

// SabreDAV server setup
// Create collections
$principalBackend = new Sabre\DAVACL\PrincipalBackend\PDO($pdo);
$principalCollection = new Sabre\DAVACL\PrincipalCollection($principalBackend);

$calendarBackend = new Sabre\CalDAV\Backend\PDO($pdo);
$calendarRoot = new Sabre\CalDAV\CalendarRoot($principalBackend, $calendarBackend);

// Create a root collection containing our nodes
$rootCollection = new Sabre\DAV\SimpleCollection('root', [
    $principalCollection,
    $calendarRoot
]);

// Create the server with our tree
$server = new Sabre\DAV\Server(new Sabre\DAV\Tree($rootCollection));
$server->setBaseUri('/');

// Authentication backend - Use Basic auth
class BasicPdoAuthBackend extends Sabre\DAV\Auth\Backend\AbstractBasic {
    public function __construct(private $pdo) {}
    
    protected function validateUserPass($username, $password) {
        // Verify credentials against database
        $stmt = $this->pdo->prepare('SELECT username, digesta1 FROM users WHERE username = ?');
        $stmt->execute([$username]);
        $result = $stmt->fetch(PDO::FETCH_ASSOC);
        // For development, we're storing plain text passwords
        return $result && $result['digesta1'] === $password;
    }
}

$server->addPlugin(new Sabre\DAV\Auth\Plugin(new BasicPdoAuthBackend($pdo), 'SabreDAV'));

$server->addPlugin(new Sabre\DAVACL\Plugin());
$server->addPlugin(new Sabre\CalDAV\Plugin());
$server->addPlugin(new Sabre\CalDAV\ICSExportPlugin());
$server->addPlugin(new Sabre\DAV\Browser\Plugin());
$server->addPlugin(new Sabre\DAV\Sync\Plugin());

// Start server
$server->exec();